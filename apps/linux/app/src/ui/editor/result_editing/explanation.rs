use bookie_app_services::result_editability::ResultRefusal;

pub(super) fn refusal_explanation(refusal: ResultRefusal) -> String {
    match refusal {
        ResultRefusal::NotOneSelect => crate::tr!("Only the rows of one SELECT can be edited"),
        ResultRefusal::MoreThanOneTable => {
            crate::tr!("Rows from more than one table, a subquery or a CTE can't be edited")
        }
        ResultRefusal::Collapsed => crate::tr!("Grouped, distinct or computed rows can't be edited"),
        ResultRefusal::Untraced => crate::tr!("Only rows read straight from a table can be edited"),
        ResultRefusal::NoPrimaryKey => crate::tr!("Rows of a table without a primary key can't be edited"),
        ResultRefusal::KeyNotShown => {
            crate::tr!("The result leaves out part of the table's primary key, so its rows can't be edited")
        }
        ResultRefusal::ColumnsChanged => {
            crate::tr!("The table changed since the query ran. Run it again to edit its rows.")
        }
        ResultRefusal::ReadOnlySession => crate::tr!("This connection is read-only."),
        ResultRefusal::OpenTransaction => crate::tr!("Rows read inside an open transaction can't be edited here"),
        ResultRefusal::OtherDatabase => {
            crate::tr!("Rows from another database than the connection's can't be edited here")
        }
        ResultRefusal::OtherResult => crate::tr!("Only the result that owns these edits can be changed"),
        ResultRefusal::Checking => crate::tr!("Checking whether these rows can be edited…"),
        ResultRefusal::ColumnsUnread => crate::tr!("The table's columns couldn't be read, so its rows can't be edited"),
        ResultRefusal::MultiResult => crate::tr!("Rows can be edited when the run returns one result"),
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
