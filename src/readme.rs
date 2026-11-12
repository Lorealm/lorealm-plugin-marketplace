/// Replaces any instances of relative paths with absolute ones.
///
/// `[the guide](docs/guide.md)` becomes:
/// `[the guide](https://github.com/<repo>/blob/<tag>/docs/guide.md)`
pub fn ensure_absolute_paths(text: &str, repository: &str, tag: &str) -> String {
    let pages = format!("{repository}/blob/{tag}/");
    let files = format!(
        "{}/{tag}/",
        repository.replace("https://github.com/", "https://raw.githubusercontent.com/")
    );

    let mut out = String::with_capacity(text.len());
    let mut fenced = false;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if fenced {
            out.push_str(line);
            continue;
        }
        let mut rest = line;
        let mut done = 0;
        while let Some(at) = rest.find("](") {
            let (before, after) = rest.split_at(at + 2);
            out.push_str(before);
            let target_len = after.find([')', ' ', '\n']).unwrap_or(after.len());
            let target = &after[..target_len];
            if is_relative(target) {
                let image = opens_an_image(&line[..done + at]);
                out.push_str(if image { &files } else { &pages });
                out.push_str(target.trim_start_matches("./").trim_start_matches('/'));
            } else {
                out.push_str(target);
            }
            done += at + 2 + target_len;
            rest = &after[target_len..];
        }
        out.push_str(rest);
    }
    out
}

fn is_relative(target: &str) -> bool {
    !target.is_empty()
        && !target.starts_with(['#', '<'])
        && !target.starts_with("//")
        && !target.contains(':')
}

/// Whether the `[` matching the `]` that `before` ends just short of is an
/// image's, as in `![alt`.
fn opens_an_image(before: &str) -> bool {
    let mut depth = 1;
    for (i, c) in before.char_indices().rev() {
        match c {
            ']' => depth += 1,
            '[' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return before[..i].ends_with('!');
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::ensure_absolute_paths;

    fn rewritten(text: &str) -> String {
        ensure_absolute_paths(text, "https://github.com/jane/hello", "v0.1.0")
    }

    #[test]
    fn relative_targets_point_at_the_release() {
        assert_eq!(
            rewritten("See [the guide](docs/guide.md) and ![a shot](./shot.png \"Shot\").\n"),
            "See [the guide](https://github.com/jane/hello/blob/v0.1.0/docs/guide.md) and ![a shot](https://raw.githubusercontent.com/jane/hello/v0.1.0/shot.png \"Shot\").\n"
        );
    }

    #[test]
    fn an_image_inside_a_link_keeps_both_kinds() {
        assert_eq!(
            rewritten("[![shot](shot.png)](docs/big.md)"),
            "[![shot](https://raw.githubusercontent.com/jane/hello/v0.1.0/shot.png)](https://github.com/jane/hello/blob/v0.1.0/docs/big.md)"
        );
    }

    #[test]
    fn everything_else_is_left_alone() {
        let text =
            "[site](https://example.com), [top](#top), [mail](mailto:a@b.c)\n```\n[x](y)\n```\n";

        assert_eq!(rewritten(text), text);
    }
}
