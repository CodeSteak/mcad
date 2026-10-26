//! Expressions, functions and lists with OpenSCAD semantics. Expected values were
//! checked against OpenSCAD 2021.01 (`echo` output).

mod common;
use common::*;

#[test]
fn number_formatting() {
    assert_eq!(ev("1/3, 2/3, 100000, 1234567, 123456, 1e20"), "0.333333, 0.666667, 100000, 1.23457e+6, 123456, 1e+20");
    assert_eq!(ev("1e-5, 1.5e-7, 0.1+0.2, -0, 1e100, 3.0, 12345.678"), "0.00001, 1.5e-7, 0.3, 0, 1e+100, 3, 12345.7");
    assert_eq!(ev("1e-6, 0.0000123, 999999.5, 999999, 99999.95"), "1e-6, 0.0000123, 1e+6, 999999, 99999.9");
    assert_eq!(ev("-1/0, 0/0, 1e6, 0.000123456, -0.00001234"), "-inf, nan, 1e+6, 0.000123456, -0.00001234");
}

#[test]
fn arithmetic() {
    assert_eq!(ev("1 + 2 * 3, 2 ^ 10, -2 ^ 2, 7 % 3, -7 % 3, 7.5 % 2"), "7, 1024, -4, 1, -1, 1.5");
    assert_eq!(ev("[1,2,3] + [1,2], [1,2] * [3,4], 2 / [1,2], [[1,2],[3,4]] * [1,1]"), "[2, 4], 11, [2, 1], [3, 7]");
    assert_eq!(ev("[1,2]*[[1,2],[3,4]], [[1,2,3],[4,5,6]] * [[1,2],[3,4],[5,6]]"), "[7, 10], [[22, 28], [49, 64]]");
    assert_eq!(
        ev("-[1,2], [1,2]-[1], [1,2]*2, 3*[1,[2]], [1,2,3] / 2"),
        "[-1, -2], [0], [2, 4], [3, [6]], [0.5, 1, 1.5]"
    );
    assert_eq!(ev("1 % 0, (-8)^(1/3), 0^0"), "nan, nan, 1");
}

#[test]
fn undefined_operations_warn() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = run("echo(\"x\" * 2, undef + 1, true + 1, 1 + [1]);");
    assert_eq!(echoes(&out), ["undef, undef, undef, undef"]);
    assert!(has_warning(&out, "undefinierte Operation (String * Zahl)"));
    assert!(has_warning(&out, "undefinierte Operation (undef + Zahl)"));
    assert!(has_warning(&out, "undefinierte Operation (Bool + Zahl)"));
    assert!(has_warning(&out, "undefinierte Operation (Zahl + Vektor)"));
}

#[test]
fn comparisons_and_logic() {
    assert_eq!(
        ev("[1,2] < [1,3], [1,2] < [1], [2] > [1,5], \"b\" > \"a\", true > false"),
        "true, false, true, true, true"
    );
    assert_eq!(
        ev("[1,2,3] == [1,2,3.0], [1,2] != [1,2], undef == undef, 1 == true, [] == undef"),
        "true, false, true, false, false"
    );
    assert_eq!(
        ev("true && 1, 0 || \"\", !0, 1 ? \"y\" : \"n\", !\"\", ![], ![0]"),
        "true, false, true, \"y\", true, true, false"
    );
    assert_eq!(ev("0/0 == 0/0, [] == [], \"\" == []"), "false, true, false");
}

#[test]
fn trig_is_exact_at_special_angles() {
    assert_eq!(
        ev("sin(30), sin(30) == 0.5, cos(60) == 0.5, sin(180), cos(90), tan(45) == 1"),
        "0.5, true, true, 0, 0, true"
    );
    assert_eq!(
        ev("sin(-30), cos(180), cos(120), sin(150), sin(360), cos(270), tan(90), tan(135)"),
        "-0.5, -1, -0.5, 0.5, 0, 0, inf, -1"
    );
    assert_eq!(ev("atan2(1,1), acos(0.5), asin(1), atan(1), atan2(0,-1)"), "45, 60, 90, 45, 180");
    assert_eq!(ev("sin(60), tan(30), sin(45)"), "0.866025, 0.57735, 0.707107");
}

#[test]
fn math_functions() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    assert_eq!(
        ev("round(2.5), round(-2.5), floor(-1.5), ceil(1.2), sign(-3), abs(-2), pow(2,10)"),
        "3, -3, -2, 2, -1, 2, 1024"
    );
    assert_eq!(ev("ln(10), log(100), exp(1), sqrt(2), log(2, 8)"), "2.30259, 2, 2.71828, 1.41421, 3");
    assert_eq!(
        ev("min(3,1,2), max([3,1,2]), norm([3,4]), cross([1,0,0],[0,1,0]), cross([1,2],[3,4])"),
        "1, 3, 5, [0, 0, 1], -2"
    );
    assert_eq!(ev("min(5), max([5]), norm([])"), "5, 5, 0");
    let out = run("echo(min([]), max(1, \"a\"), len(5), norm(5));");
    assert_eq!(echoes(&out), ["undef, undef, undef, undef"]);
    assert!(has_warning(&out, "min(): Argumente sind weder Zahlen noch ein Vektor"));
    assert!(has_warning(&out, "len(): Argument ist weder Vektor noch String"));
}

#[test]
fn lists_and_strings() {
    assert_eq!(
        ev("len(\"abc\"), len([1,2]), concat([1],[2,3],4), [1,2,3][1.7], \"abc\"[1], [1,2][5]"),
        "3, 2, [1, 2, 3, 4], 2, \"b\", undef"
    );
    assert_eq!(
        ev("concat(\"ab\", \"cd\"), concat([1], \"ab\"), concat(1, 2), concat()"),
        "[\"ab\", \"cd\"], [1, \"ab\"], [1, 2], []"
    );
    assert_eq!(ev("[1,2,3].x, [1,2].z, \"abc\".x, [0:5][2]"), "1, undef, undef, 5");
    assert_eq!(
        ev("str(\"a\", 1, [1,\"b\"]), str(1/3), str(1e20), str(), str(undef), str(-0)"),
        "\"a1[1, \"b\"]\", \"0.333333\", \"1e+20\", \"\", \"undef\", \"0\""
    );
    assert_eq!(
        ev("chr(65), chr([65,66]), ord(\"A\"), chr(228), len(\"ä\"), ord(\"ä\"), ord(\"\")"),
        "\"A\", \"AB\", 65, \"ä\", 1, 228, undef"
    );
    assert_eq!(
        ev(
            "lookup(1.5, [[1,10],[2,20]]), lookup(5,[[1,10],[2,20]]), lookup(0, [[1,10],[2,20]]), lookup(1.5, [[2,20],[1,10]])"
        ),
        "15, 20, 10, 15"
    );
}

#[test]
fn search_function() {
    assert_eq!(
        ev("search(\"a\",\"abcabc\"), search(3,[1,2,3]), search([1,2],[[1,\"a\"],[2,\"b\"]])"),
        "[0], [2], [0, 1]"
    );
    assert_eq!(
        ev("search(\"b\", [[\"a\",1],[\"b\",2]], 0), search(\"ab\", \"abcab\", 0), search(\"x\", \"abc\")"),
        "[[1]], [[0, 3], [1, 4]], []"
    );
    assert_eq!(
        ev(
            "search(0, [[0,1],[0,2],[3,4]], 0), search(0, [[0,1],[0,2],[3,4]], 1), search([\"a\"], [[\"a\",1],[\"b\",2],[\"a\",3]], 0)"
        ),
        "[0, 1], [0], [[0, 2]]"
    );
}

#[test]
fn ranges() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    assert_eq!(ev("[1:3], [0:0.5:2], [0:-1:-3]"), "[1 : 1 : 3], [0 : 0.5 : 2], [0 : -1 : -3]");
    assert_eq!(ev("[for (i=[0:0.1:1]) i]"), "[0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1]");
    assert_eq!(
        ev("[for(i=[0.5:2]) i], [for(i=[1:0.3:2]) i], [for (i=[0:-1:-2]) i], [for (i=[0:1:-1]) i]"),
        "[0.5, 1.5], [1, 1.3, 1.6, 1.9], [0, -1, -2], []"
    );
    let out = run("echo([for (i=[3:1]) i]);");
    assert_eq!(echoes(&out), ["[1, 2, 3]"]);
    assert!(has_warning(&out, "veraltet"));
}

#[test]
fn comprehensions() {
    assert_eq!(ev("[for (i=[0:3]) i*i]"), "[0, 1, 4, 9]");
    assert_eq!(ev("[for (i=[0:2], j=[0:1]) [i,j]]"), "[[0, 0], [0, 1], [1, 0], [1, 1], [2, 0], [2, 1]]");
    assert_eq!(
        ev("[each [1,2], each [3:4]], [for (i=0; i<3; i=i+1) i], [for (i=[1:5]) if (i%2==0) i else -i]"),
        "[1, 2, 3, 4], [0, 1, 2], [-1, 2, -3, 4, -5]"
    );
    assert_eq!(
        ev(
            "[for (i = [0:2]) let(j = i*2) j], [for (a = [1,2], b = [a:3]) [a,b]], [if (true) 1], [if (false) 1], [each \"ab\"]"
        ),
        "[0, 2, 4], [[1, 1], [1, 2], [1, 3], [2, 2], [2, 3]], [1], [], [\"a\", \"b\"]"
    );
    assert_eq!(ev("[for (c = \"abc\") c], [for (x = 5) x], [for (x = undef) x]"), "[\"a\", \"b\", \"c\"], [5], []");
    assert_eq!(ev("[for (i = 0, s = 0; i < 4; i = i + 1, s = s + i) s]"), "[0, 1, 3, 6]");
    assert_eq!(ev("[1, 2, 3,]"), "[1, 2, 3]");
}

#[test]
fn let_and_function_literals() {
    assert_eq!(ev("let(a=2, b=a*3) a+b"), "8");
    assert_eq!(ev_in("f = function(x) x*2;", "f(3), (function(y) y+1)(2), is_function(f)"), "6, 3, true");
    assert_eq!(ev_in("function adder(n) = function(x) x + n; add5 = adder(5);", "add5(1)"), "6");
    assert_eq!(ev_in("function fac(n) = n <= 1 ? 1 : n * fac(n - 1);", "fac(10), fac(20)"), "3.6288e+6, 2.4329e+18");
    assert_eq!(ev_in("function g(a, b = 2) = let (c = a + b) c * c;", "g(1), g(1, 3), g(b = 1, a = 1)"), "9, 16, 4");
}

#[test]
fn type_predicates() {
    assert_eq!(
        ev("is_num(1), is_list([]), is_string(\"a\"), is_undef(undef), is_bool(true), is_function(function(x) x)"),
        "true, true, true, true, true, true"
    );
    assert_eq!(
        ev("is_list(\"a\"), is_num(true), is_undef(0), is_function(undef), is_string([\"a\"]), is_bool(0)"),
        "false, false, false, false, false, false"
    );
    assert_eq!(ev("is_num(0/0), is_num(1/0), version(), version_num()"), "false, true, [2021, 1, 0], 2.02101e+7");
}

#[test]
fn echo_named_args_and_nesting() {
    assert_eq!(ev("a=1, b=\"x\", [1, [2, [3]]]"), "a = 1, b = \"x\", [1, [2, [3]]]");
}

#[test]
fn variables_are_lazy_and_last_assignment_wins() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "echo(s); s = 5; cube(s);";
    let out = clean(src);
    assert_eq!(echoes(&out), ["5"]);
    let out = run("a = b; b = 5; echo(a);");
    assert_eq!(echoes(&out), ["5"]);
    let out = run("x = 1;\necho(x);\nx = 2;");
    assert_eq!(echoes(&out), ["2"]);
    assert!(has_warning(&out, "`x` aus Zeile 1 wird in Zeile 3 überschrieben"));
}

#[test]
fn recursive_definition_is_detected() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = run("a = b + 1; b = a + 1; echo(a);");
    assert!(has_warning(&out, "rekursive Definition"));
}

#[test]
fn unknown_names_warn() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = run("echo(nope, f(1));");
    assert_eq!(echoes(&out), ["undef, undef"]);
    assert!(has_warning(&out, "unbekannte Variable `nope`"));
    assert!(has_warning(&out, "unbekannte Funktion `f`"));
}

#[test]
fn block_scopes_shadow() {
    let out = clean("x = 1; translate([0,0,0]) { x = 2; echo(x); } echo(x);");
    assert_eq!(echoes(&out), ["2", "1"]);
}

#[test]
fn module_parameters_and_defaults() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let src = "module m(a = 1, b = 2) { echo(a, b); } m(); m(5); m(b = 3); m(1, 2);";
    let out = clean(src);
    assert_eq!(echoes(&out), ["1, 2", "5, 2", "1, 3", "1, 2"]);
    // As in OpenSCAD, a default is evaluated where the module is defined: it does not
    // see the other parameters (`b = a * 2` uses the file's `a`).
    let out = run("a = 10; module m(a, b = a * 2) echo(b = b); m(1);");
    assert_eq!(echoes(&out), ["b = 20"]);
    let out = run("module m(a) { echo(a); } m(1, 2); m(c = 3);");
    assert!(has_warning(&out, "zu viele Argumente"));
    assert!(has_warning(&out, "hat keinen Parameter `c`"));
}

#[test]
fn special_variables_are_dynamic() {
    let src = "
        module show() echo($fn, $t, $preview);
        module outer() { $fn = 7; show(); }
        function f() = $fn;
        show();
        outer();
        show($fn = 5);
        echo(f(), f($fn = 3));
        $fn = 9;
        echo(f());
    ";
    let out = clean(src);
    assert_eq!(echoes(&out), ["9, 0, true", "7, 0, true", "5, 0, true", "9, 3", "9"]);
}

#[test]
fn children_count_and_selection() {
    let src = "
        module m() { echo($children); children(1); children([0, 2]); }
        m() { cube(1); sphere(1); cylinder(h = 1, r = 1); }
    ";
    let out = clean(src);
    assert_eq!(echoes(&out), ["3"]);
    assert_eq!(dump(&out), "group\n  m\n    group\n      sphere\n    group\n      box\n      cylinder\n");
}

#[test]
fn children_see_caller_scope_and_module_specials() {
    let src = "
        module ring(n) { for (i = [0:n-1]) rotate([0, 0, i * 360 / n]) children(); }
        r = 7;
        ring(3) translate([r, 0, 0]) echo($fn, r);
        module set() { $fn = 6; children(); }
        set() echo($fn);
    ";
    let out = clean(src);
    assert_eq!(echoes(&out), ["0, 7", "0, 7", "0, 7", "6"]);
}

#[test]
fn rands_is_deterministic() {
    let a = run("echo(rands(0, 10, 3));");
    let b = run("echo(rands(0, 10, 3));");
    assert_eq!(echoes(&a), echoes(&b));
    assert_eq!(ev("rands(0,10,3,42) == rands(0,10,3,42), len(rands(0, 1, 5)), rands(0, 1, 0)"), "true, 5, []");
    let v = ev("rands(2, 3, 20)");
    for x in v.trim_matches(['[', ']']).split(", ") {
        let x: f64 = x.parse().unwrap();
        assert!((2.0..3.0).contains(&x));
    }
}

#[test]
fn assert_failure_aborts() {
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = run("x = 1; assert(x == 2, \"x must be 2\"); echo(\"after\");");
    assert!(out.aborted);
    assert!(has_error(&out, "assert 'x == 2' fehlgeschlagen: x must be 2"));
    assert!(echoes(&out).is_empty());
    let out = clean("v = assert(true) 3; echo(v);");
    assert_eq!(echoes(&out), ["3"]);
}

#[test]
fn echo_expression() {
    let out = clean("v = echo(\"hi\", 2) 3; echo(v);");
    assert_eq!(echoes(&out), ["\"hi\", 2", "3"]);
}

#[test]
fn pi_constant() {
    assert_eq!(ev("PI"), "3.14159");
}

#[test]
fn messages_in_english() {
    let out = run("echo(nope, f(1));");
    assert!(has_warning(&out, "unknown variable `nope`"));
    assert!(has_warning(&out, "unknown function `f`"));
    let out = run("x = 1; assert(x == 2, \"x must be 2\"); echo(\"after\");");
    assert!(has_error(&out, "assert 'x == 2' failed: x must be 2"));
    let out = run("x = [1]; echo(x[0](2));");
    assert!(has_warning(&out, "number is not callable"), "{:?}", out.diagnostics);
    let _de = cad_i18n::scoped(cad_i18n::Lang::De);
    let out = run("x = [1]; echo(x[0](2));");
    assert!(has_warning(&out, "Zahl ist nicht aufrufbar"), "{:?}", out.diagnostics);
}
