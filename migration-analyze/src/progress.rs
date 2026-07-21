use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Shared multi-layer progress display for CLI commands.
pub struct ProgressDisplay {
    multi: MultiProgress,
}

impl ProgressDisplay {
    pub fn new() -> Self {
        Self {
            multi: MultiProgress::new(),
        }
    }

    /// Create a determinate progress bar for "x of N files" tracking.
    pub fn add_bar(&self, total: u64, prefix: &str) -> ProgressBar {
        let pb = ProgressBar::new(total);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{prefix:.bold} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("##-"),
        );
        pb.set_prefix(prefix.to_string());
        self.multi.add(pb)
    }

    /// Create an indeterminate spinner for steps without known count.
    pub fn add_spinner(&self, msg: &str) -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        pb.set_message(msg.to_string());
        self.multi.add(pb)
    }
}
