use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{CodeActionParams, CodeActionResponse};

use super::FloeLsp;

impl FloeLsp {
    pub(super) async fn handle_code_action(
        &self,
        _params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>> {
        // No quickfix code actions are currently offered. The former
        // "add inferred return type" action existed to unstick the E010
        // "exported function must declare a return type" diagnostic;
        // that diagnostic no longer exists — Floe infers and exports
        // the return type directly, including narrow tsgo-resolved
        // shapes that a user couldn't write manually.
        Ok(None)
    }
}
