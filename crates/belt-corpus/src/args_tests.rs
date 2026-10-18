use super::*;

fn args(text: &str) -> Vec<OsString> {
    text.split(' ').map(OsString::from).collect()
}

#[test]
fn no_arguments_is_usage() {
    assert_eq!(parse(&[]), Err(ArgsError::Usage));
}

#[test]
fn missing_truth_means_six_empty_slots() {
    let options = parse(&args("dir --dump")).unwrap();

    assert_eq!(options.truth, vec!["-"; 6]);
    assert!(options.dump);
    assert_eq!((options.from.as_str(), options.to.as_str()), ("000000", "999999"));
    assert_eq!(options.confirm, None);
}

#[test]
fn excluded_frames_are_a_list_and_none_by_default() {
    assert!(parse(&args("dir")).unwrap().exclude.is_empty());
    assert_eq!(parse(&args("dir --exclude 105609-130,110258-932")).unwrap().exclude, ["105609-130", "110258-932"]);
}

#[test]
fn truth_drops_empty_entries() {
    let options = parse(&args("dir rock,,knife.bone,-")).unwrap();

    assert_eq!(options.truth, vec!["rock", "knife.bone", "-"]);
}

#[test]
fn confirm_takes_a_count_or_defaults_to_two() {
    assert_eq!(parse(&args("dir --confirm 3")).unwrap().confirm, Some(3));
    assert_eq!(parse(&args("dir --confirm")).unwrap().confirm, Some(2));
    // The value slot holds the next flag, which does not parse: the C# fallback applies.
    assert_eq!(parse(&args("dir --confirm --dump")).unwrap().confirm, Some(2));
    assert_eq!(parse(&args("dir --confirm -4")).unwrap().confirm, Some(0));
}

#[test]
fn numeric_flags_parse_invariantly_and_reject_garbage() {
    let options = parse(&args("dir --departure 0.05 --coarse 100")).unwrap();
    assert_eq!(options.departure, Some(0.05));
    assert_eq!(options.coarse, Some(100));
    assert_eq!(parse(&args("dir --coarse -5")).unwrap().coarse, Some(0));

    assert!(matches!(parse(&args("dir --departure x")), Err(ArgsError::Invalid { flag: "--departure", .. })));
}

#[test]
fn lists_split_on_commas() {
    let options = parse(&args("dir --categories Weapon,,Attire --skin-categories tool --all-skins --skins --flat")).unwrap();

    assert_eq!(options.categories, Some(vec!["Weapon".to_owned(), "Attire".to_owned()]));
    assert_eq!(options.skin_categories, Some(vec!["tool".to_owned()]));
    assert!(options.all_skins && options.skins && options.flat);
}

#[cfg(windows)]
#[test]
fn non_unicode_paths_pass_through_and_text_values_are_rejected() {
    use std::os::windows::ffi::OsStringExt as _;
    let lone_surrogate = OsString::from_wide(&[0x61, 0xD800]);

    let options = parse(&[lone_surrogate.clone(), "--montage".into(), lone_surrogate.clone()]).unwrap();
    assert_eq!(options.directory.as_os_str(), lone_surrogate.as_os_str());
    assert_eq!(options.montage.as_deref().map(|p| p.as_os_str()), Some(lone_surrogate.as_os_str()));

    assert!(matches!(parse(&["dir".into(), lone_surrogate.clone()]), Err(ArgsError::Invalid { flag: "truth", .. })));
    assert!(matches!(parse(&["dir".into(), "--from".into(), lone_surrogate]), Err(ArgsError::Invalid { flag: "--from", .. })));
}

#[test]
fn case_folding_is_ordinal_ignore_case() {
    assert!(eq_ignore_case("Weapon", "wEAPON"));
    assert!(!eq_ignore_case("Weapon", "Weapons"));
}
