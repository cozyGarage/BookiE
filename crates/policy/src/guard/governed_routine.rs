use super::*;

fn routine_target(routine: &bookie_core::RoutineRef) -> Vec<String> {
    let arguments = routine.identity_arguments.as_deref().unwrap_or_default();
    vec![format!("routine:{}.{}({arguments})", routine.schema, routine.name)]
}

fn found_rows(lookup: &bookie_core::RoutineLookup) -> u64 {
    u64::from(matches!(lookup, bookie_core::RoutineLookup::Found(_)))
}

impl PolicyGuard {
    pub(super) async fn governed_routine_definition(
        &self,
        routine: &bookie_core::RoutineRef,
    ) -> Result<bookie_core::RoutineLookup, DriverError> {
        let operation = self.metadata_operation("ROUTINE DEFINITION", routine_target(routine));
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read("ROUTINE DEFINITION", self.inner.routine_definition(routine))
            .await;
        let rows = result.as_ref().ok().map(found_rows);
        self.audit_read_result(&operation, start, &result, rows).await?;
        result
    }

    pub(super) async fn governed_routine_definition_controlled(
        &self,
        routine: &bookie_core::RoutineRef,
        control: &OperationControl,
    ) -> Result<bookie_core::RoutineLookup, DriverError> {
        check_pre_dispatch(control)?;
        let operation = self.metadata_operation("ROUTINE DEFINITION", routine_target(routine));
        self.prepare_governed_read(&operation).await?;
        let start = Instant::now();
        let result = self
            .caught_read(
                "ROUTINE DEFINITION",
                self.inner.routine_definition_controlled(routine, control),
            )
            .await;
        let rows = result.as_ref().ok().map(found_rows);
        self.audit_controlled_read_result(&operation, start, &result, rows)
            .await?;
        result
    }
}
