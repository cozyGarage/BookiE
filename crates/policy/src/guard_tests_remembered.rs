use super::*;
use crate::approval::{ApprovalGrant, ApprovalGrants, RememberingApprovalSink};

struct CountingSink {
    prompts: Arc<AtomicUsize>,
    answer: ApprovalOutcome,
}

#[async_trait]
impl ApprovalSink for CountingSink {
    async fn request(&self, _request: ApprovalRequest) -> ApprovalOutcome {
        self.prompts.fetch_add(1, Ordering::SeqCst);
        self.answer
    }
}

fn policy_for(environment: &str) -> PolicyConfig {
    let mut policy = PolicyConfig::default();
    policy
        .environments
        .entry(environment.into())
        .or_default()
        .human_approve_writes = Some(true);
    policy
}

struct Rig {
    guard: PolicyGuard,
    prompts: Arc<AtomicUsize>,
    executes: Arc<AtomicUsize>,
    audit: Arc<SequenceAuditSink>,
}

fn rig(principal: Principal, environment: Environment, grants: &Arc<ApprovalGrants>) -> Rig {
    let prompts = Arc::new(AtomicUsize::new(0));
    let executes = Arc::new(AtomicUsize::new(0));
    let audit = Arc::new(SequenceAuditSink::new(vec![]));
    let sink = RememberingApprovalSink::new(
        CountingSink {
            prompts: prompts.clone(),
            answer: ApprovalOutcome::AllowAndRemember,
        },
        grants.clone(),
    );
    let name = if environment == Environment::Prod {
        "prod"
    } else {
        "local"
    };
    let guard = PolicyGuard::new(
        connection(executes.clone(), Arc::new(AtomicUsize::new(0))),
        context(
            principal,
            environment,
            policy_for(name),
            Arc::new(sink),
            audit.clone(),
            Arc::new(AuditState::new()),
        ),
    );
    Rig {
        guard,
        prompts,
        executes,
        audit,
    }
}

fn last_approval(rig: &Rig) -> AuditApprovalOutcome {
    rig.audit
        .events
        .lock()
        .expect("audit events")
        .last()
        .expect("terminal outcome")
        .approval_outcome
}

const WRITE: &str = "DELETE FROM items WHERE id = 1";

#[tokio::test]
async fn a_remembered_approval_skips_the_next_prompt_for_the_same_rule_and_is_audited() {
    let grants = Arc::new(ApprovalGrants::default());
    let rig = rig(Principal::human_gui(), Environment::Local, &grants);

    rig.guard.execute(WRITE).await.expect("first write");
    assert_eq!(last_approval(&rig), AuditApprovalOutcome::ApprovedAndRemembered);
    rig.guard.execute(WRITE).await.expect("second write");

    assert_eq!(rig.prompts.load(Ordering::SeqCst), 1);
    assert_eq!(rig.executes.load(Ordering::SeqCst), 2);
    assert_eq!(last_approval(&rig), AuditApprovalOutcome::ApprovedByGrant);
    assert_eq!(grants.list().len(), 1);
}

#[tokio::test]
async fn a_remembered_approval_does_not_cover_another_principal() {
    let grants = Arc::new(ApprovalGrants::default());
    let person = rig(Principal::human_gui(), Environment::Local, &grants);
    person.guard.execute(WRITE).await.expect("person");
    let agent = rig(
        Principal::Agent {
            token: "token".into(),
            client: None,
            model: None,
        },
        Environment::Local,
        &grants,
    );

    let _ = agent.guard.execute(WRITE).await;

    assert_eq!(agent.prompts.load(Ordering::SeqCst), 1, "the agent is asked itself");
}

#[tokio::test]
async fn a_production_connection_keeps_asking_and_never_stores_a_grant() {
    let grants = Arc::new(ApprovalGrants::default());
    let rig = rig(Principal::human_gui(), Environment::Prod, &grants);

    rig.guard.execute(WRITE).await.expect("first write");
    rig.guard.execute(WRITE).await.expect("second write");

    assert_eq!(rig.prompts.load(Ordering::SeqCst), 2);
    assert_eq!(last_approval(&rig), AuditApprovalOutcome::Approved);
    assert!(grants.list().is_empty());
}

#[tokio::test]
async fn revoking_a_grant_restores_the_prompt() {
    let grants = Arc::new(ApprovalGrants::default());
    let rig = rig(Principal::human_gui(), Environment::Local, &grants);
    rig.guard.execute(WRITE).await.expect("first write");
    let grant = grants.list().remove(0);

    assert!(grants.revoke(&grant));
    rig.guard.execute(WRITE).await.expect("write after revoke");

    assert_eq!(rig.prompts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn closing_a_connection_removes_only_its_grants() {
    let grants = Arc::new(ApprovalGrants::default());
    let rig = rig(Principal::human_gui(), Environment::Local, &grants);
    rig.guard.execute(WRITE).await.expect("write");
    let other = ApprovalGrant {
        connection_id: Uuid::new_v4(),
        connection_name: "other".into(),
        rule: "other".into(),
        principal: Principal::human_gui(),
    };
    grants.add(other.clone());
    let remembered = grants.list().into_iter().find(|grant| grant != &other).expect("grant");

    grants.revoke_connection(remembered.connection_id);

    assert_eq!(grants.list(), vec![other]);
}
