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
            self.inner.write_line("")?;
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
