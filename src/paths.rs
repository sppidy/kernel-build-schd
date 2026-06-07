use camino::{Utf8Path, Utf8PathBuf};

pub fn lexical_child_of(path: &Utf8Path, roots: &[Utf8PathBuf]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}
