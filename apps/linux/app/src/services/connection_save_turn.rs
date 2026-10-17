static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn take() -> tokio::sync::MutexGuard<'static, ()> {
    TURN.lock().await
}

#[cfg(test)]
pub fn is_held() -> bool {
    TURN.try_lock().is_err()
}
