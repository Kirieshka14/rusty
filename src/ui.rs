use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

pub fn success(msg: &str) {
    println!("{} {}", "✔".bright_green().bold(), msg);
}

pub fn info(msg: &str) {
    println!("{} {}", "ℹ".bright_blue().bold(), msg);
}

pub fn warn(msg: &str) {
    println!("{} {}", "⚠".bright_yellow().bold(), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} {}", "✖".bright_red().bold(), msg);
}

pub fn header(title: &str) {
    println!("\n{}", title.bright_cyan().bold());
    println!("{}", "─".repeat(title.len()).bright_black());
}

pub fn create_download_progress(total_bytes: u64, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:30.cyan/blue}] {bytes}/{total_bytes} ({eta}) - {msg}")
            .unwrap()
            .progress_chars("━╸─"),
    );
    pb.set_message(msg.to_string());
    pb
}

pub fn create_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb.set_message(msg.to_string());
    pb
}
