use bookie_app_services::result_editability::ResultRefusal;

pub(super) fn refusal_explanation(refusal: ResultRefusal) -> String {
    match refusal {
        ResultRefusal::NotOneSelect => bookie_app_services::tr!("Only the rows of one SELECT can be edited"),
        ResultRefusal::MoreThanOneTable => {
            bookie_app_services::tr!("Rows from more than one table, a subquery or a CTE can't be edited")
        }
        ResultRefusal::Collapsed => bookie_app_services::tr!("Grouped, distinct or computed rows can't be edited"),
        ResultRefusal::Untraced => bookie_app_services::tr!("Only rows read straight from a table can be edited"),
        ResultRefusal::NoPrimaryKey => bookie_app_services::tr!("Rows of a table without a primary key can't be edited"),
        ResultRefusal::KeyNotShown => {
            bookie_app_services::tr!("The result leaves out part of the table's primary key, so its rows can't be edited")
        }
        ResultRefusal::ColumnsChanged => {
            bookie_app_services::tr!("The table changed since the query ran. Run it again to edit its rows.")
        }
        ResultRefusal::ReadOnlySession => bookie_app_services::tr!("This connection is read-only."),
        ResultRefusal::OpenTransaction => bookie_app_services::tr!("Rows read inside an open transaction can't be edited here"),
        ResultRefusal::OtherDatabase => {
            bookie_app_services::tr!("Rows from another database than the connection's can't be edited here")
        }
        ResultRefusal::OtherResult => bookie_app_services::tr!("Only the result that owns these edits can be changed"),
        ResultRefusal::Checking => bookie_app_services::tr!("Checking whether these rows can be edited…"),
        ResultRefusal::ColumnsUnread => bookie_app_services::tr!("The table's columns couldn't be read, so its rows can't be edited"),
        ResultRefusal::MultiResult => bookie_app_services::tr!("Rows can be edited when the run returns one result"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use bookie_app_services::result_editability::ResultRefusal;

    use super::refusal_explanation;

    #[test]
    fn every_refusal_explains_itself_once() {
        let all = [
            ResultRefusal::NotOneSelect,
            ResultRefusal::MoreThanOneTable,
            ResultRefusal::Collapsed,
            ResultRefusal::Untraced,
            ResultRefusal::NoPrimaryKey,
            ResultRefusal::KeyNotShown,
            ResultRefusal::ColumnsChanged,
            ResultRefusal::ReadOnlySession,
            ResultRefusal::OpenTransaction,
            ResultRefusal::OtherDatabase,
            ResultRefusal::OtherResult,
            ResultRefusal::Checking,
            ResultRefusal::ColumnsUnread,
            ResultRefusal::MultiResult,
        ];
        let texts: HashSet<String> = all.iter().copied().map(refusal_explanation).collect();
        assert_eq!(texts.len(), all.len());
    }
}
