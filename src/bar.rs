pub const TEMPLATE_DEFAULT: &str =
    "{msg}\n[{elapsed_precise}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})";
pub fn get_progress_bar(
    manager: &kyuri::Manager,
    len: u64,
    template: &str,
    url: &url::Url,
) -> kyuri::Bar {
    manager.create_bar(len, &format!("Downloading {}", url), template)
}
