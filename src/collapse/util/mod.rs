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
#[allow(clippy::cognitive_complexity)]
pub(crate) fn fix_partially_demangled_rust_symbol(symbol: &str) -> Cow<str> {
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

#[cfg(test)]
mod tests {
    macro_rules! t {
        ($a:expr, $b:expr) => {
            assert!(ok($a, $b))
        };
    }

    macro_rules! t_unchanged {
        ($a:expr) => {
            assert!(ok_unchanged($a))
        };
    }

    fn ok(sym: &str, expected: &str) -> bool {
        let result = super::fix_partially_demangled_rust_symbol(sym);
        if result == expected {
            true
        } else {
            println!("\n{}\n!=\n{}\n", result, expected);
            false
        }
    }

    fn ok_unchanged(sym: &str) -> bool {
        let result = super::fix_partially_demangled_rust_symbol(sym);
        if result == sym {
            true
        } else {
            println!("{} should have been unchanged, but got {}", sym, result);
            false
        }
    }

    #[test]
    fn fix_partially_demangled_rust_symbols() {
        t!(
            "std::sys::unix::fs::File::open::hb90e1c1c787080f0",
            "std::sys::unix::fs::File::open"
        );
        t!("_$LT$std..fs..ReadDir$u20$as$u20$core..iter..traits..iterator..Iterator$GT$::next::hc14f1750ca79129b", "<std::fs::ReadDir as core::iter::traits::iterator::Iterator>::next");
        t!("rg::search_parallel::_$u7b$$u7b$closure$u7d$$u7d$::_$u7b$$u7b$closure$u7d$$u7d$::h6e849b55a66fcd85", "rg::search_parallel::_{{closure}}::_{{closure}}");
        t!(
            "_$LT$F$u20$as$u20$alloc..boxed..FnBox$LT$A$GT$$GT$::call_box::h8612a2a83552fc2d",
            "<F as alloc::boxed::FnBox<A>>::call_box"
        );
        t!(
            "_$LT$$RF$std..fs..File$u20$as$u20$std..io..Read$GT$::read::h5d84059cf335c8e6",
            "<&std::fs::File as std::io::Read>::read"
        );
        t!(
            "_$LT$std..thread..JoinHandle$LT$T$GT$$GT$::join::hca6aa63e512626da",
            "<std::thread::JoinHandle<T>>::join"
        );
        t!(
            "std::sync::mpsc::shared::Packet$LT$T$GT$::recv::hfde2d9e28d13fd56",
            "std::sync::mpsc::shared::Packet<T>::recv"
        );
        t!("crossbeam_utils::thread::ScopedThreadBuilder::spawn::_$u7b$$u7b$closure$u7d$$u7d$::h8fdc7d4f74c0da05", "crossbeam_utils::thread::ScopedThreadBuilder::spawn::_{{closure}}");
    }

    #[test]
    fn fix_partially_demangled_rust_symbol_on_fully_mangled_symbols() {
        t_unchanged!("_ZN4testE");
        t_unchanged!("_ZN4test1a2bcE");
        t_unchanged!("_ZN7inferno10flamegraph5merge6frames17hacfe2d67301633c2E");
        t_unchanged!("_ZN3std2rt19lang_start_internal17h540c897fe52ba9c5E");
        t_unchanged!("_ZN116_$LT$core..str..pattern..CharSearcher$LT$$u27$a$GT$$u20$as$u20$core..str..pattern..ReverseSearcher$LT$$u27$a$GT$$GT$15next_match_back17h09d544049dd719bbE");
        t_unchanged!("_ZN3std5panic12catch_unwind17h0562757d03ff60b3E");
        t_unchanged!("_ZN3std9panicking3try17h9c1cbc5599e1efbfE");
    }

    #[test]
    fn fix_partially_demangled_rust_symbol_on_fully_demangled_symbols() {
        t_unchanged!("std::sys::unix::fs::File::open");
        t_unchanged!("<F as alloc::boxed::FnBox<A>>::call_box");
        t_unchanged!("<std::fs::ReadDir as core::iter::traits::iterator::Iterator>::next");
        t_unchanged!("<rg::search::SearchWorker<W>>::search_impl");
        t_unchanged!("<grep_searcher::searcher::glue::ReadByLine<'s, M, R, S>>::run");
        t_unchanged!("<alloc::raw_vec::RawVec<T, A>>::reserve_internal");
    }
}
