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

#[cfg(test)]
mod tests {
    pub const TEMPLATE_SIMPLE: &str = "{msg}\n{bytes}/{total_bytes}";
    use super::*;
    use std::io::{Read, Seek};
    use test_log::test;

    #[test]
    fn test_pb_to_file() {
        let memfd_name = std::ffi::CString::new("test_pb_to_file").unwrap();
        let memfd_fd =
            nix::sys::memfd::memfd_create(&memfd_name, nix::sys::memfd::MemFdCreateFlag::empty())
                .unwrap();
        let memfd_writer: std::fs::File = memfd_fd.into();
        let mut memfd_writer_clone = memfd_writer.try_clone().unwrap();
        let progressbar_manager =
            kyuri::Manager::new(std::time::Duration::from_secs(1)).with_file(memfd_writer);
        let pb1 = get_progress_bar(
            &progressbar_manager,
            10,
            TEMPLATE_SIMPLE,
            &url::Url::parse("http://d1.example.com").unwrap(),
        );
        let pb2 = get_progress_bar(
            &progressbar_manager,
            10,
            TEMPLATE_SIMPLE,
            &url::Url::parse("http://d2.example.com").unwrap(),
        );

        pb1.set_pos(2);
        pb2.set_pos(3);
        progressbar_manager.draw(true);
        pb1.set_pos(5);
        pb2.set_pos(7);

        std::mem::drop(progressbar_manager);
        memfd_writer_clone
            .seek(std::io::SeekFrom::Start(0))
            .unwrap();
        let mut output = String::new();
        memfd_writer_clone.read_to_string(&mut output).unwrap();
        assert_eq!(
            output,
            r#"Downloading http://d1.example.com/
0 B/10 B
Downloading http://d1.example.com/
0 B/10 B
Downloading http://d2.example.com/
0 B/10 B
Downloading http://d1.example.com/
2 B/10 B
Downloading http://d2.example.com/
3 B/10 B
Downloading http://d1.example.com/
5 B/10 B
Downloading http://d2.example.com/
7 B/10 B
"#
        );
    }
}
