use super::*;

impl GridView {
    pub(crate) fn pace_frame(&mut self, window: &mut Window) {
        let last = self.visible_count().saturating_sub(1);
        let Some(pace) = self.pace.as_mut() else { return };
        let target = (f64::from(pace.step) / f64::from(pace.steps) * last as f64) as usize;
        self.scroll.scroll_to_item_strict(target, ScrollStrategy::Top);
        pace.step += 1;
        if pace.step <= pace.steps {
            window.request_animation_frame();
            return;
        }
        let elapsed = pace.begun.elapsed().as_millis();
        let steps = pace.steps;
        self.pace = None;
        self.measuring = false;
        self.report_scroll(steps, elapsed, true);
    }

    pub(crate) fn report_scroll(&self, steps: u32, elapsed: u128, with_frames: bool) {
        let mut sorted: Vec<f64> = self.frame_intervals.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
        sorted.sort_by(f64::total_cmp);
        let pick = |q: f64| {
            sorted
                .get(((sorted.len() as f64 - 1.0) * q) as usize)
                .copied()
                .unwrap_or(0.0)
        };
        let frames = if with_frames {
            format!(
                " frames={} frame_p50_ms={:.2} frame_p95_ms={:.2} frame_p99_ms={:.2} frame_max_ms={:.2}",
                sorted.len(),
                pick(0.5),
                pick(0.95),
                pick(0.99),
                pick(1.0)
            )
        } else {
            String::new()
        };
        println!("PROTOTYPE_SCROLL_DONE steps={steps} elapsed_ms={elapsed}{frames}");
    }

    pub(crate) fn start_autoscroll(&mut self, steps: u32, cx: &mut Context<Self>) {
        if std::env::var("BOOKIE_PROTOTYPE_AUTOSCROLL_SCOPE").as_deref() == Ok("continuous") {
            self.continuous = true;
        }
        let frame_paced = std::env::var("BOOKIE_PROTOTYPE_AUTOSCROLL_PACING").as_deref() == Ok("frame");
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            if frame_paced {
                this.update(cx, |this, cx| {
                    this.frame_intervals.clear();
                    this.last_frame = None;
                    this.measuring = true;
                    this.pace = Some(Pace {
                        step: 0,
                        steps,
                        begun: Instant::now(),
                    });
                    cx.notify();
                })
                .ok();
                return;
            }
            let begun = Instant::now();
            for step in 0..=steps {
                let moved = this.update(cx, |this, cx| {
                    let last = this.visible_count().saturating_sub(1);
                    let target = (f64::from(step) / f64::from(steps) * last as f64) as usize;
                    this.scroll.scroll_to_item_strict(target, ScrollStrategy::Top);
                    cx.notify();
                });
                if moved.is_err() {
                    return;
                }
                cx.background_executor().timer(Duration::from_millis(62)).await;
            }
            let elapsed = begun.elapsed().as_millis();
            this.update(cx, |this, _| this.report_scroll(steps, elapsed, false))
                .ok();
        })
        .detach();
    }
}
