use super::{RunMessage, SqlEditorInput};

pub(super) fn blocks_execution(input: &SqlEditorInput) -> bool {
    matches!(
        input,
        SqlEditorInput::Run(
            RunMessage::Run
                | RunMessage::RunWithParameters { .. }
                | RunMessage::RunWithLongerTimeout
                | RunMessage::RunAtCursor
                | RunMessage::Explain
                | RunMessage::ShowTemporaryTable { .. }
        )
    )
}
