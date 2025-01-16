// A workaround term struct for indicatif to display bars in redirected files,
// as nobody knows how to design this feature inside indicatif properly.
// See: - https://github.com/console-rs/indicatif/issues/87
//      - https://github.com/console-rs/indicatif/issues/530

use console::Term;
use indicatif::TermLike;

#[derive(Debug)]
pub struct AlternativeTerm {
    inner: Term,
}

impl TermLike for AlternativeTerm {
    fn clear_line(&self) -> std::io::Result<()> {
        if self.inner.is_term() {
            self.inner.clear_line()
        } else {
            // self.inner.write_line("")?;
            Ok(())
        }
    }

    fn flush(&self) -> std::io::Result<()> {
        self.inner.flush()
    }

    fn move_cursor_down(&self, n: usize) -> std::io::Result<()> {
        if self.inner.is_term() {
            self.inner.move_cursor_down(n)
        } else {
            // Here is a very dirty hack to make the output compat when redirecting to a file:
            // the progress bar would trigger "cursor down" only once for each print,
            // so just print an empty line when cursor is down, and ignore clearing line request.
            self.inner.write_line("")?;
            Ok(())
        }
    }

    fn move_cursor_left(&self, n: usize) -> std::io::Result<()> {
        if self.inner.is_term() {
            self.inner.move_cursor_left(n)
        } else {
            Ok(())
        }
    }

    fn move_cursor_right(&self, n: usize) -> std::io::Result<()> {
        if self.inner.is_term() {
            self.inner.move_cursor_right(n)
        } else {
            Ok(())
        }
    }

    fn move_cursor_up(&self, n: usize) -> std::io::Result<()> {
        if self.inner.is_term() {
            self.inner.move_cursor_up(n)
        } else {
            Ok(())
        }
    }

    fn write_line(&self, s: &str) -> std::io::Result<()> {
        self.inner.write_line(s)
    }

    fn width(&self) -> u16 {
        self.inner.size().1
    }

    fn write_str(&self, s: &str) -> std::io::Result<()> {
        self.inner.write_str(s)
    }
}

impl AlternativeTerm {
    pub fn buffered_stdout() -> Self {
        Self {
            inner: Term::buffered_stdout(),
        }
    }
}

// For testing convenience
pub const TEMPLATE_DEFAULT: &str = "{msg}\n[{elapsed_precise}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})";
pub fn set_download_progress_bar(pb: &indicatif::ProgressBar, template: &str, url: &url::Url) {
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template(template)
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.set_message(format!("Downloading {}", url));
}

#[cfg(test)]
mod tests {
    pub const TEMPLATE_SIMPLE: &str = "{msg}\n{bytes}/{total_bytes}";
    use std::io::{Read, Seek};
    use super::*;
    use test_log::test;

    #[test]
    fn test_pb_to_file() {
        let devnull_reader = std::fs::File::open("/dev/null").unwrap();
        let memfd_name = std::ffi::CString::new("test_pb_to_file").unwrap();
        let memfd_fd =
            nix::sys::memfd::memfd_create(&memfd_name, nix::sys::memfd::MemFdCreateFlag::empty())
                .unwrap();
        let memfd_writer: std::fs::File = memfd_fd.into();
        let mut memfd_writer_clone = memfd_writer.try_clone().unwrap();
        let term = Term::read_write_pair(devnull_reader, memfd_writer);
        assert!(term.is_term() == false);
        let term = AlternativeTerm { inner: term };
        let mprogress = indicatif::MultiProgress::with_draw_target(
            indicatif::ProgressDrawTarget::term_like_with_hz(Box::new(term), 1),
        );
        let pb1 = mprogress.add(indicatif::ProgressBar::new(10));
        set_download_progress_bar(&pb1, TEMPLATE_SIMPLE, &url::Url::parse("http://d1.example.com").unwrap());
        let pb2 = mprogress.add(indicatif::ProgressBar::new(10));
        set_download_progress_bar(&pb2, TEMPLATE_SIMPLE, &url::Url::parse("http://d2.example.com").unwrap());

        pb1.set_position(2);
        pb2.set_position(3);
        pb1.set_position(5);
        pb2.set_position(7);
        
        std::mem::drop(mprogress);
        memfd_writer_clone.seek(std::io::SeekFrom::Start(0)).unwrap();
        let mut output = String::new();
        memfd_writer_clone.read_to_string(&mut output).unwrap();
        assert_eq!(output, r#"Downloading http://d1.example.com/
0 B/10 B                                                                        
Downloading http://d1.example.com/
0 B/10 B
Downloading http://d2.example.com/
0 B/10 B                                                                        


Downloading http://d1.example.com/
2 B/10 B
Downloading http://d2.example.com/
0 B/10 B                                                                        


Downloading http://d1.example.com/
2 B/10 B
Downloading http://d2.example.com/
3 B/10 B                                                                        


Downloading http://d1.example.com/
5 B/10 B
Downloading http://d2.example.com/
3 B/10 B                                                                        


Downloading http://d1.example.com/
5 B/10 B
Downloading http://d2.example.com/
7 B/10 B                                                                        "#);
    }
}
