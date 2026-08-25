pub fn path_looks_like_a_transient_install(path: &str) -> bool {
    path.contains("AppTranslocation")
        || path.contains("/Downloads/")
        || path.contains("/.Trash/")
        || path.starts_with("/Volumes/")
}

pub fn path_is_a_stable_app_install(path: &str) -> bool {
    let trimmed = path.trim_end_matches('/');
    trimmed.ends_with("/Applications/Rustle.app")
}

#[cfg(test)]
mod tests {
    use super::{path_is_a_stable_app_install, path_looks_like_a_transient_install};

    #[test]
    fn translocation_is_transient() {
        assert!(path_looks_like_a_transient_install(
            "/private/var/folders/xx/T/AppTranslocation/ABC/d/Rustle.app"
        ));
    }

    #[test]
    fn downloads_and_dmg_are_transient() {
        assert!(path_looks_like_a_transient_install(
            "/Users/nick/Downloads/Rustle.app"
        ));
        assert!(path_looks_like_a_transient_install("/Volumes/Rustle/Rustle.app"));
    }

    #[test]
    fn applications_is_stable() {
        assert!(path_is_a_stable_app_install("/Applications/Rustle.app"));
        assert!(path_is_a_stable_app_install(
            "/Users/nick/Applications/Rustle.app"
        ));
        assert!(!path_is_a_stable_app_install(
            "/private/var/folders/xx/T/AppTranslocation/ABC/d/Rustle.app"
        ));
        assert!(!path_looks_like_a_transient_install("/Applications/Rustle.app"));
    }
}
