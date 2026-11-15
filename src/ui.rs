use anyhow::Result;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{stdin, stdout, Write};

pub fn success(msg: &str) {
    println!("{} {}", "✔".bright_green().bold(), msg);
}

pub fn info(msg: &str) {
    println!("{} {}", "::".bright_blue().bold(), msg);
}

pub fn warn(msg: &str) {
    println!("{} {}", "warning:".bright_yellow().bold(), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} {}", "error:".bright_red().bold(), msg);
}

pub fn header(title: &str) {
    println!("\n{}", title.bright_cyan().bold());
    println!("{}", "─".repeat(title.len()).bright_black());
}

pub fn paru_log(msg: &str) {
    println!("{} {}", "::".bright_cyan().bold(), msg);
}

pub fn paru_step(current: usize, total: usize, action: &str) {
    println!(
        "{} [{}/{}] {}",
        "::".bright_cyan().bold(),
        current.to_string().bright_yellow(),
        total.to_string().bright_yellow(),
        action.bold()
    );
}

pub fn paru_detail(label: &str, value: &str) {
    println!("   {:<18} {}", label.bright_black(), value);
}

pub fn paru_audit(ok: bool, text: &str) {
    if ok {
        println!("   {} {}", "[✓]".bright_green().bold(), text);
    } else {
        println!("   {} {}", "[!]".bright_yellow().bold(), text);
    }
}

pub fn confirm(prompt: &str, default_yes: bool) -> Result<bool> {
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{} {} {} ", "::".bright_yellow().bold(), prompt.bold(), suffix.bright_black());
    stdout().flush()?;

    let mut line = String::new();
    stdin().read_line(&mut line)?;
    let trimmed = line.trim();

    if trimmed.is_empty() {
        return Ok(default_yes);
    }

    match trimmed.to_lowercase().as_str() {
        "y" | "yes" | "д" | "да" => Ok(true),
        "n" | "no" | "н" | "нет" => Ok(false),
        _ => Ok(default_yes),
    }
}

pub fn create_download_progress(total_bytes: u64, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("   {spinner:.green} [{elapsed_precise}] [{bar:30.cyan/blue}] {bytes}/{total_bytes} ({eta}) - {msg}")
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
            .template("   {spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb.set_message(msg.to_string());
    pb
}
