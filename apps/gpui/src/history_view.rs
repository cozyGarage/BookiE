use gpui::Context;

use crate::GridView;

static PRUNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl GridView {
    pub(crate) fn reload_history(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let recent = cx
                .background_executor()
                .spawn(async move {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string())?;
                    runtime.block_on(async {
                        if !PRUNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                            let _ = bookie_app_services::statement_history::prune().await;
                        }
                        bookie_app_services::statement_history::recent(8).await
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(recent) = recent {
                    this.history = recent;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
