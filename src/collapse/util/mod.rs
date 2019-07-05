use std::borrow::Cow;

const RUST_HASH_LENGTH: usize = 17;

// Rust hashes are hex digits with an `h` prepended.
fn is_rust_hash(s: &str) -> bool {
    s.starts_with('h') && s[1..].chars().all(|c| c.is_digit(16))
}

/// Demangles partially demangled Rust symbols that were demangled incorrectly by profilers like
/// `sample` and `DTrace`.
///
/// For example:
///     `_$LT$grep_searcher..searcher..glue..ReadByLine$LT$$u27$s$C$$u20$M$C$$u20$R$C$$u20$S$GT$$GT$::run::h30ecedc997ad7e32`
/// becomes
///     `<grep_searcher::searcher::glue::ReadByLine<'s, M, R, S>>::run`
///
/// Non-Rust symobols, or Rust symbols that are already demangled, will be returned unchanged.
///
/// Based on code in https://github.com/alexcrichton/rustc-demangle/blob/master/src/legacy.rs
pub(crate) fn fix_partially_demangled_rust_symbol<'a>(symbol: &'a str) -> Cow<'a, str> {
    // If there's no trailing Rust hash just return the symbol as is.
    if symbol.len() < RUST_HASH_LENGTH || !is_rust_hash(&symbol[symbol.len() - RUST_HASH_LENGTH..])
    {
        return Cow::Borrowed(symbol);
    }

    // Strip off trailing hash.
    let mut rest = &symbol[..symbol.len() - RUST_HASH_LENGTH];

    if rest.ends_with("::") {
        rest = &rest[..rest.len() - 2];
    }

    if rest.starts_with("_$") {
        rest = &rest[1..];
    }

    let mut demangled = String::new();

    while !rest.is_empty() {
        if rest.starts_with('.') {
            if let Some('.') = rest[1..].chars().next() {
                demangled.push_str("::");
                rest = &rest[2..];
            } else {
                demangled.push_str(".");
                rest = &rest[1..];
            }
        } else if rest.starts_with('$') {
            macro_rules! demangle {
                ($($pat:expr => $demangled:expr,)*) => ({
                    $(if rest.starts_with($pat) {
                        demangled.push_str($demangled);
                        rest = &rest[$pat.len()..];
                        } else)*
                    {
                        demangled.push_str(rest);
                        break;
                    }

                })
            }

            demangle! {
                "$SP$" => "@",
                "$BP$" => "*",
                "$RF$" => "&",
                "$LT$" => "<",
                "$GT$" => ">",
                "$LP$" => "(",
                "$RP$" => ")",
                "$C$" => ",",
                "$u7e$" => "~",
                "$u20$" => " ",
                "$u27$" => "'",
                "$u3d$" => "=",
                "$u5b$" => "[",
                "$u5d$" => "]",
                "$u7b$" => "{",
                "$u7d$" => "}",
                "$u3b$" => ";",
                "$u2b$" => "+",
                "$u21$" => "!",
                "$u22$" => "\"",
            }
        } else {
            let idx = match rest.char_indices().find(|&(_, c)| c == '$' || c == '.') {
                None => rest.len(),
                Some((i, _)) => i,
            };
            demangled.push_str(&rest[..idx]);
            rest = &rest[idx..];
        }
    }

    Cow::Owned(demangled)
}
