use bookie_core::import::CsvImportOptions;

#[derive(Default)]
pub(super) struct PreviewQueue {
    generation: u64,
    running: bool,
    pending: Option<PreviewWork>,
    closed: bool,
}

pub(super) struct PreviewWork {
    pub generation: u64,
    pub options: CsvImportOptions,
    pub auto_detect: bool,
}

pub(super) struct PreviewCompletion {
    pub apply: bool,
    pub next: Option<PreviewWork>,
}

impl PreviewQueue {
    pub fn request(&mut self, options: CsvImportOptions, auto_detect: bool) -> Option<PreviewWork> {
        if self.closed {
            return None;
        }
        self.generation = self.generation.wrapping_add(1).max(1);
        let work = PreviewWork {
            generation: self.generation,
            options,
            auto_detect,
        };
        if self.running {
            self.pending = Some(work);
            return None;
        }
        self.running = true;
        Some(work)
    }

    pub fn finish(&mut self, generation: u64) -> PreviewCompletion {
        if self.closed {
            self.running = false;
            self.pending = None;
            return PreviewCompletion {
                apply: false,
                next: None,
            };
        }
        let next = self.pending.take();
        let apply = generation == self.generation && next.is_none();
        if next.is_none() {
            self.running = false;
        }
        PreviewCompletion { apply, next }
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.generation = self.generation.wrapping_add(1).max(1);
        self.pending = None;
    }

    pub fn current_generation(&self) -> u64 {
        self.generation
    }

    pub fn is_ready(&self, generation: u64) -> bool {
        !self.closed && !self.running && self.generation == generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bookie_core::export::CsvDelimiter;

    #[test]
    fn keeps_only_the_latest_options_while_one_preview_is_running() {
        let mut queue = PreviewQueue::default();
        let first = queue.request(CsvImportOptions::default(), true).unwrap();
        assert!(
            queue
                .request(
                    CsvImportOptions {
                        delimiter: CsvDelimiter::Semicolon,
                        ..CsvImportOptions::default()
                    },
                    false,
                )
                .is_none()
        );
        assert!(
            queue
                .request(
                    CsvImportOptions {
                        delimiter: CsvDelimiter::Tab,
                        ..CsvImportOptions::default()
                    },
                    false,
                )
                .is_none()
        );
        let completion = queue.finish(first.generation);
        assert!(!completion.apply);
        let latest = completion.next.unwrap();
        assert_eq!(latest.options.delimiter, CsvDelimiter::Tab);
        assert!(!latest.auto_detect);
        let completion = queue.finish(latest.generation);
        assert!(completion.apply);
        assert!(completion.next.is_none());
        assert!(queue.is_ready(latest.generation));
    }

    #[test]
    fn closing_invalidates_running_and_pending_previews() {
        let mut queue = PreviewQueue::default();
        let running = queue.request(CsvImportOptions::default(), true).unwrap();
        queue.request(CsvImportOptions::default(), false);
        queue.close();
        let completion = queue.finish(running.generation);
        assert!(!completion.apply);
        assert!(completion.next.is_none());
        assert!(!queue.is_ready(running.generation));
        assert!(queue.request(CsvImportOptions::default(), false).is_none());
    }
}
