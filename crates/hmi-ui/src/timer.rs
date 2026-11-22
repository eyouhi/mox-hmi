pub async fn sleep_ms(milliseconds: u64) {
    #[cfg(target_arch = "wasm32")]
    {
        gloo_timers::future::TimeoutFuture::new(milliseconds.min(u32::MAX as u64) as u32).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        futures_timer::Delay::new(std::time::Duration::from_millis(milliseconds)).await;
    }
}
