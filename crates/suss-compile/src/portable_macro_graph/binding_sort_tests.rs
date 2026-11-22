use super::*;

#[test]
fn ordered_namespace_names_do_not_repeat_merge_work() {
    let owned = (0..1024).map(|i| format!("n{i:04}")).collect::<Vec<_>>();
    let mut names = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let before = names.clone();
    let mut scratch = names.clone();
    let mut work = 0;
    sort_binding_names(&mut names, &mut scratch, &mut |units| {
        work += units;
        assert!(
            work < 8192,
            "ordered names must take linear comparison work"
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(names, before);
    assert!(
        work >= names.len() - 1,
        "comparison work must remain charged"
    );
}

#[test]
fn partially_ordered_names_keep_duplicates_and_utf16_order() {
    let original = [
        "",
        "a",
        "a",
        "z",
        "\u{10000}",
        "\u{e000}",
        "",
        "A",
        "z",
        "\u{10000}",
        "\u{e000}",
    ];
    let expected = [
        "",
        "",
        "A",
        "a",
        "a",
        "z",
        "z",
        "\u{10000}",
        "\u{10000}",
        "\u{e000}",
        "\u{e000}",
    ];
    let mut names = original.to_vec();
    let mut scratch = names.clone();
    sort_binding_names(&mut names, &mut scratch, &mut |_| Ok(())).unwrap();
    assert_eq!(names, expected);
}
