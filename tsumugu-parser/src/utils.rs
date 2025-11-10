use anyhow::Result;
use tracing::warn;

pub fn again<T, E: std::fmt::Debug, F: FnMut() -> Result<T, E>>(
    mut f: F,
    retries: usize,
) -> Result<T, E> {
    for attempt in 0..=retries {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if attempt == retries => return Err(e),
            Err(e) => {
                warn!("Error: {:?}. retry {}/{}", e, attempt + 1, retries);
            }
        }
    }
    unreachable!()
}

pub fn relative_str_process(relative: &str) -> String {
    let mut r = relative.to_string();
    if r.starts_with('/') {
        warn!("unexpected / at the beginning of relative ({r})");
    } else {
        r.insert(0, '/');
    }
    if r.len() != 1 {
        if r.ends_with('/') {
            warn!("unexpected / at the end of relative ({r})")
        } else {
            r.push('/')
        }
    }
    r
}

pub fn relative_to_str(relative: &[String], filename: Option<&str>) -> String {
    let r = relative.join("/");
    let r = relative_str_process(&r);

    // here r already has / at the end
    match filename {
        None => r,
        Some(filename) => {
            assert!(!filename.starts_with('/') && !filename.ends_with('/'));
            format!("{}{}", r, filename)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

    #[test]
    fn test_relative() {
        let mut relative: Vec<String> = vec![];
        assert_eq!(relative_to_str(&relative, None), "/");
        relative.push("debian".to_string());
        assert_eq!(relative_to_str(&relative, None), "/debian/");
        relative.push("dists".to_string());
        assert_eq!(relative_to_str(&relative, None), "/debian/dists/");
        relative.push("bookworm".to_string());
        assert_eq!(relative_to_str(&relative, None), "/debian/dists/bookworm/");
        assert_eq!(
            relative_to_str(&relative, Some("Release")),
            "/debian/dists/bookworm/Release"
        );
    }
}
