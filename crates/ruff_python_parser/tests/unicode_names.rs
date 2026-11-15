use std::cell::RefCell;

use ruff_python_ast::{Expr, Mod};
use ruff_python_parser::{
    Mode, ParseOptions, ParseRuntime, UnicodeNameProvider, UnicodeNameProviderUnavailable,
    parse_unchecked_with_runtime,
};

#[derive(Debug)]
struct Provider<'a> {
    events: &'a RefCell<Vec<String>>,
    available: bool,
}

impl ParseRuntime for Provider<'_> {}

impl UnicodeNameProvider for Provider<'_> {
    fn prepare(&self) -> Result<(), UnicodeNameProviderUnavailable> {
        self.events.borrow_mut().push("prepare".into());
        if self.available {
            Ok(())
        } else {
            Err(UnicodeNameProviderUnavailable)
        }
    }

    fn lookup(&self, name: &[u8]) -> Option<u32> {
        self.events
            .borrow_mut()
            .push(String::from_utf8_lossy(name).into_owned());
        (name == b"CUSTOM NAME").then_some(u32::from('Z'))
    }
}

#[test]
fn named_escape_uses_borrowed_provider() {
    let events = RefCell::default();
    let provider = Provider {
        events: &events,
        available: true,
    };
    let parsed = parse_unchecked_with_runtime(
        r"'\N{CUSTOM NAME}'",
        ParseOptions::from(Mode::Expression),
        &provider,
    );
    assert_eq!(parsed.errors(), []);
    let Mod::Expression(module) = parsed.syntax() else {
        panic!("expected expression")
    };
    let Expr::StringLiteral(literal) = module.body.as_ref() else {
        panic!("expected string")
    };
    assert_eq!(literal.value.to_str(), "Z");
    assert_eq!(*events.borrow(), ["prepare", "CUSTOM NAME"]);
}

#[test]
fn only_decoded_named_escapes_prepare_provider() {
    for source in [
        r"r'\N{A}'",
        r"b'\N{A}'",
        r"'\\N{A}'",
        "# \\N{A}\npass",
        r"'\u0041'",
    ] {
        let events = RefCell::default();
        let provider = Provider {
            events: &events,
            available: false,
        };
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &provider);
        assert!(
            parsed.errors().is_empty(),
            "{source}: {:?}",
            parsed.errors()
        );
        assert!(events.borrow().is_empty(), "{source}");
    }
}

#[test]
fn preparation_precedes_name_validation() {
    for source in [
        r"'\N'",
        r"'\N{}'",
        r"'\N{missing'",
        r"'\N{CUSTOM NAME}'",
        r"f'\N{CUSTOM NAME}'",
        r"t'\N{CUSTOM NAME}'",
    ] {
        let events = RefCell::default();
        let provider = Provider {
            events: &events,
            available: false,
        };
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &provider);
        assert_eq!(
            parsed.errors()[0].error.to_string(),
            "(unicode error) \\N escapes not supported (can't load unicodedata module)",
            "{source}"
        );
        assert!(events.borrow().iter().all(|event| event == "prepare"));
        assert!(!events.borrow().is_empty());
    }
}

#[test]
fn provider_error_obeys_syntax_error_order() {
    for (source, provider_first) in [(r"x = ; '\N{A}'", false), (r"'\N{A}'; x = ;", true)] {
        let events = RefCell::default();
        let provider = Provider {
            events: &events,
            available: false,
        };
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &provider);
        assert_eq!(
            parsed.errors()[0]
                .error
                .to_string()
                .contains("can't load unicodedata"),
            provider_first,
            "{source}"
        );
    }
}

#[test]
fn syntax_error_does_not_import_during_recovery() {
    let events = RefCell::default();
    let provider = Provider {
        events: &events,
        available: false,
    };
    let parsed = parse_unchecked_with_runtime(r"x = ; '\N{A}'", Mode::Module.into(), &provider);
    assert_ne!(parsed.errors(), []);
    assert!(events.borrow().is_empty());
}

#[test]
fn invalid_grammar_still_prepares_named_escapes() {
    for source in [
        r"(a '\N{A}')",
        r"pass if '\N{A}' else 1",
        r"break if '\N{A}' else 1",
        r"continue if 1 else '\N{A}'",
        r"case '\N{A}': pass",
    ] {
        let events = RefCell::default();
        let provider = Provider {
            events: &events,
            available: false,
        };
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &provider);
        assert!(
            !events.borrow().is_empty(),
            "{source}: {:?}",
            parsed.errors()
        );
        assert!(
            parsed.errors()[0]
                .error
                .to_string()
                .contains("can't load unicodedata"),
            "{source}: {:?}",
            parsed.errors()
        );
    }
}

#[derive(Debug)]
struct WarningRuntime<'a>(Provider<'a>);

impl UnicodeNameProvider for WarningRuntime<'_> {
    fn prepare(&self) -> Result<(), UnicodeNameProviderUnavailable> {
        self.0.prepare()
    }
    fn lookup(&self, name: &[u8]) -> Option<u32> {
        self.0.lookup(name)
    }
}

impl ParseRuntime for WarningRuntime<'_> {
    fn literal_decoded(
        &self,
        source: &str,
        _flags: ruff_python_ast::AnyStringFlags,
        range: ruff_text_size::TextRange,
    ) -> Result<(), ruff_python_parser::ParseError> {
        self.0.events.borrow_mut().push(format!("decoded:{source}"));
        if source == r"\q" {
            Err(ruff_python_parser::ParseError {
                error: ruff_python_parser::ParseErrorType::OtherError("warning-as-error".into()),
                location: range,
            })
        } else {
            Ok(())
        }
    }
}

#[test]
fn decoder_failure_precedes_warning_in_same_literal() {
    for source in [r"'\q\N{A}'", r"f'\q\N{A}'", r"t'\q\N{A}'"] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: false,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert!(
            parsed.errors()[0]
                .error
                .to_string()
                .contains("can't load unicodedata"),
            "{source}: {:?}",
            parsed.errors()
        );
        assert_eq!(*events.borrow(), ["prepare"]);
    }
}

#[test]
fn earlier_literal_warning_aborts_before_later_import() {
    for source in [r"'\q' '\N{A}'", r"'\q'; '\N{A}'"] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: false,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert_eq!(parsed.errors()[0].error.to_string(), "warning-as-error");
        assert_eq!(*events.borrow(), ["decoded:\\q"]);
    }
}

#[test]
fn decoder_errors_survive_invalid_grammar_backtracking() {
    for (source, reason) in [
        (r"(a '\N{UNKNOWN}')", "unknown Unicode character name"),
        (r"(a '\N{}')", "malformed \\N"),
        (r"(a '\xg')", "truncated \\x"),
    ] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: true,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert!(
            parsed.errors()[0].error.to_string().contains(reason),
            "{source}: {:?}",
            parsed.errors()
        );
    }
}

#[test]
fn invalid_interpolated_grammar_does_not_decode_outer_fragments() {
    for source in [
        r"f'\N{A}{'",
        r"f'\N{A}}'",
        r"f'\N{A}{1+}'",
        r"f'{1+}\N{A}'",
        r"t'\N{A}{1+}'",
        r"f'\q{1+}'",
    ] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: false,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert_ne!(parsed.errors(), []);
        assert!(
            events.borrow().is_empty(),
            "{source}: {:?}",
            events.borrow()
        );
    }
}

#[test]
fn inner_expression_decoding_precedes_outer_fragments() {
    for source in [
        r#"f'{"\N{A}"}{1+}'"#,
        r#"f'\N{A}{"\N{B}"}'"#,
        r#"f'\q{"\N{B}"}'"#,
    ] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: false,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert!(
            parsed.errors()[0]
                .error
                .to_string()
                .contains("can't load unicodedata"),
            "{source}: {:?}",
            parsed.errors()
        );
        assert_eq!(*events.borrow(), ["prepare"]);
        assert_eq!(
            &source[parsed.errors()[0].location],
            if source.contains('B') {
                r#""\N{B}""#
            } else {
                r#""\N{A}""#
            }
        );
    }
}

#[test]
fn tokenizer_failure_before_literal_does_not_import() {
    for source in [r"$; '\N{A}'", r"  '\N{A}'"] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: false,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert_ne!(parsed.errors(), []);
        assert!(
            events.borrow().is_empty(),
            "{source}: {:?}",
            events.borrow()
        );
    }
}

#[test]
fn decoder_failure_precedes_later_tokenizer_failure() {
    let events = RefCell::default();
    let runtime = WarningRuntime(Provider {
        events: &events,
        available: false,
    });
    let parsed = parse_unchecked_with_runtime(r"'\N{A}'; $", Mode::Module.into(), &runtime);
    assert!(
        parsed.errors()[0]
            .error
            .to_string()
            .contains("can't load unicodedata")
    );
    assert_eq!(*events.borrow(), ["prepare"]);
}

#[test]
fn format_spec_decode_is_immediate_and_preserves_exception_kind() {
    for source in [
        r"f'{1:\N{A}}'",
        r"f'{1:\N{A}'",
        r"f'{1:\N{A}}{'",
        r"f'{1:\N{A}{2+}}'",
        r"t'{1:\N{A}}'",
    ] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: false,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert!(
            matches!(
                parsed.errors()[0].error,
                ruff_python_parser::ParseErrorType::Lexical(
                    ruff_python_parser::LexicalErrorType::FormatSpecDecodeError(_)
                )
            ),
            "{source}: {:?}",
            parsed.errors()
        );
        assert_eq!(*events.borrow(), ["prepare"]);
    }
}

#[test]
fn invalid_format_spec_expression_stops_later_import() {
    let events = RefCell::default();
    let runtime = WarningRuntime(Provider {
        events: &events,
        available: false,
    });
    let parsed = parse_unchecked_with_runtime(r"f'{1:{2+}\N{A}}'", Mode::Module.into(), &runtime);
    assert_ne!(parsed.errors(), []);
    assert!(events.borrow().is_empty());
}

#[derive(Debug)]
struct BuiltinRuntime;
impl UnicodeNameProvider for BuiltinRuntime {
    fn prepare(&self) -> Result<(), UnicodeNameProviderUnavailable> {
        Ok(())
    }
    fn lookup(&self, name: &[u8]) -> Option<u32> {
        std::str::from_utf8(name)
            .ok()
            .and_then(unicode_names2::character)
            .map(u32::from)
    }
}
impl ParseRuntime for BuiltinRuntime {}

#[test]
fn runtime_preserves_valid_ast_and_tokens() {
    for source in [
        r"'\N{LATIN CAPITAL LETTER A}'",
        r"f'\N{LATIN CAPITAL LETTER A}{x!r:>3}'",
        r"f'left{{{x:>3}}}right'",
        r"f'{x:\N{LATIN CAPITAL LETTER A}}'",
        r#"f'\N{LATIN CAPITAL LETTER A}{f"nested{x}"}'"#,
        r"t'left{x:\N{LATIN CAPITAL LETTER A}}right'",
        r"rf'\N{{name}} {x}'",
    ] {
        let plain = ruff_python_parser::parse_unchecked(source, Mode::Module.into());
        let runtime = parse_unchecked_with_runtime(source, Mode::Module.into(), &BuiltinRuntime);
        assert!(
            runtime.errors().is_empty(),
            "{source}: {:?}",
            runtime.errors()
        );
        assert_eq!(plain.syntax(), runtime.syntax(), "{source}");
        assert_eq!(plain.tokens(), runtime.tokens(), "{source}");
    }
}

#[test]
fn committed_grammar_errors_do_not_decode_recovery_literals() {
    for source in [r"def f(a '\N{A}'): pass", r"def '\N{A}'", r"import '\N{A}'"] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: false,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert_ne!(parsed.errors(), []);
        assert!(
            events.borrow().is_empty(),
            "{source}: {:?}; {:?}",
            events.borrow(),
            parsed.errors()
        );
    }
}

#[test]
fn expression_grammar_can_decode_before_invalid_syntax() {
    for source in [
        r"a '\N{A}'",
        r"f(a '\N{A}')",
        r"x = 1 '\N{A}'",
        r"for '\N{A}' in x: pass",
    ] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: false,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert!(
            parsed.errors()[0]
                .error
                .to_string()
                .contains("can't load unicodedata"),
            "{source}: {:?}",
            parsed.errors()
        );
        assert_eq!(*events.borrow(), ["prepare"]);
    }
}

#[test]
fn invalid_expression_probe_is_bounded_by_grammar() {
    for (source, imports) in [
        (r"(a b '\N{A}')", false),
        (r"a b '\N{A}'", true),
        (r"def f(a=1 '\N{A}'): pass", true),
        (r"def f(a: int '\N{A}'): pass", true),
    ] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: false,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert_eq!(
            events.borrow().contains(&"prepare".to_owned()),
            imports,
            "{source}: {:?}",
            parsed.errors()
        );
    }
}

#[test]
fn legacy_probe_evaluates_star_expression_list() {
    for (source, imports) in [
        (r"a 1, '\N{A}'", true),
        (r"a 1, *x, '\N{A}'", true),
        (r"print 1, *x, '\N{A}'", true),
        (r"(a 1, *x, '\N{A}')", false),
        (r"print 1, '\N{A}'", true),
        (r"(print 1, '\N{A}')", true),
        (r"(a 1, '\N{A}')", false),
    ] {
        let events = RefCell::default();
        let runtime = WarningRuntime(Provider {
            events: &events,
            available: false,
        });
        let parsed = parse_unchecked_with_runtime(source, Mode::Module.into(), &runtime);
        assert_eq!(
            events.borrow().contains(&"prepare".to_owned()),
            imports,
            "{source}: {:?}",
            parsed.errors()
        );
    }
}
