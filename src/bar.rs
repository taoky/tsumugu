pub const TEMPLATE_DEFAULT: &str =
    "{msg}\n[{elapsed_precise}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})";
pub fn set_progress_bar(bar: &kyuri::Bar, len: u64, url: &url::Url) {
    bar.set_len(len);
    bar.set_message(&format!("Downloading {}", url));
    bar.set_template(TEMPLATE_DEFAULT);
    bar.set_pos(0);
    bar.set_visible(true);
}
