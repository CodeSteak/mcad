//! Compares `echo` output with OpenSCAD (if `openscad` is installed; otherwise the
//! test only checks that the file evaluates).

mod common;

use std::process::Command;

const SRC: &str = r#"
echo(1/3, 2/3, 100000, 1234567, 123456, 1e20, 1e-5, 1.5e-7, 0.1+0.2, -0, 1e100, 3.0, 12345.678);
echo(1e-6, 1.5e-6, 2e-6, 1.234567e-6, 0.0000123, 999999.5, 999999, 99999.95, 0.1234565);
echo(sin(30), sin(30)==0.5, cos(60)==0.5, sin(180), cos(90), tan(45), tan(45)==1, sin(45));
echo(sin(-30), cos(180), cos(120), sin(150), sin(360), cos(270), tan(135), sin(720+30), cos(-60));
echo(sin(60), cos(30), sin(210), cos(240), tan(30), tan(60), tan(-45), tan(180), tan(225));
echo(atan2(1,1), acos(0.5), asin(1), atan(1), atan2(0,-1), atan2(-1, 0), acos(-1));
echo([1,2,3]+[1,2], [1,2]*[3,4], 2/[1,2], [[1,2],[3,4]]*[1,1], [1,2]*[[1,2],[3,4]]);
echo([[1,2,3],[4,5,6]] * [[1,2],[3,4],[5,6]], [1,2,3] * [[1,0,0],[0,1,0],[0,0,1]]);
echo("abc", str("a", 1, [1,"b"]), [1:3], [0:0.5:2], undef, true, -1/0, 0/0);
echo(len("abc"), len([1,2]), concat([1],[2,3],4), [1,2,3][1.7], "abc"[1], [1,2][5]);
echo(round(2.5), round(-2.5), floor(-1.5), ceil(1.2), sign(-3), abs(-2), pow(2,10), 2^10, -2^2, 7%3, -7%3, 7.5%2);
echo(-7 % -3, 7 % -3, 5.5 % -2, -0.5 % 1, 2^0.5, 0^0, round(0.5), round(-0.5), floor(1e300));
echo(min(3,1,2), max([3,1,2]), norm([3,4]), cross([1,0,0],[0,1,0]), lookup(1.5, [[1,10],[2,20]]), lookup(5,[[1,10],[2,20]]));
echo(lookup(0, [[1,10],[2,20]]), lookup(1.5, [[2,20],[1,10]]), lookup(3, [[1,1]]), lookup(10, [[1,10],[2,20],[3,30]]));
echo(chr(65), chr([65,66]), ord("A"), search("a","abcabc"), search(3,[1,2,3]), search([1,2],[[1,"a"],[2,"b"]]), search("b", [["a",1],["b",2]], 0));
echo(search(0, [[0,1],[0,2],[3,4]], 0), search(0, [[0,1],[0,2],[3,4]], 1), search("ab", "abcab", 0), search(["a"], [["a",1],["b",2],["a",3]], 0));
echo([for (i=[0:3]) i*i], [for (i=[0:2], j=[0:1]) [i,j]], [each [1,2], each [3:4]], [for (i=0; i<3; i=i+1) i], [for (i=[1:5]) if (i%2==0) i else -i]);
echo([for (i = [0:2]) let(j = i*2) j], [for (a = [1,2], b = [a:3]) [a,b]], [if (true) 1], [if (false) 1], [each "ab"]);
echo(let(a=2, b=a*3) a+b, [1,2,3]==[1,2,3], "a" < "b", [1]<[2], 5 == 5.0, is_num(1), is_list([]), is_string("a"), is_undef(undef), is_bool(true));
f = function(x) x*2; echo(f(3), (function(y) y+1)(2));
function fac(n) = n<=1 ? 1 : n*fac(n-1); echo(fac(10), fac(20));
echo(str(1/3), str(1e20), str(1234567), str(1.0), str([0.1, 2]), version(), version_num());
echo(a=1, b="x", [1, [2, [3]]]);
echo(ln(10), log(100), exp(1), sqrt(2), 1e6, 1e15, 1e16, 123456789012, 0.000123456, 1.23456789e-10);
echo([0:-1:-3], [for (i=[0:0.1:1]) i], [for(i=[0.5:2]) i], [for(i=[1:0.3:2]) i]);
echo(true && 1, 0 || "", !0, 1 ? "y" : "n", -[1,2], [1,2]-[1], [1,2]*2, 3*[1,[2]]);
echo(norm([1,2,3]), cross([1,2],[3,4]), rands(0,1,0), [1,2] < [1,3], [1,2] < [1], [2] > [1,5], true > false);
echo([1,2,3].x, [1,2].z, "abc".x, [for (c = "abc") c], [for (x = 5) x], [for (x = undef) x]);
echo(chr(228), len("ä"), ord("ä"), floor(2), 1e-7, 0.1, 1/7*1e6, 1e21);
echo(is_num(0/0), is_num(1/0), 0/0 == 0/0, [] == [], "" == [], concat("ab", "cd"), concat([1], "ab"));
echo([1,2,3] - [1,1,1], [1,2,3] / 2, [1,2,3] == [1,2,3.0], [1,2] != [1,2], undef == undef, 1 == true, 0 == false);
echo(str(), str(undef), str(true), str(1, 2, "x"), str(-0), str([]), str([[]]), str(1e-7), ord(""), ord("ab"));
echo(is_list("a"), is_num(true), is_undef(0), is_function(undef), is_string(["a"]), is_bool(0), max([5]), min(5));
echo(1e-5 * 3, 33333.33, 0.3333333, -123456.7, -0.00001234, abs(-0), sign(0), floor(-0.5), ceil(-0.5));
module m(a = 1, b = 2) { echo(a = a, b = b, n = $children); }
m(); m(3); m(b = 5) cube(1);
function sum(v, i = 0) = i >= len(v) ? 0 : v[i] + sum(v, i + 1);
echo(sum([1, 2, 3, 4]), [for (i = [0:4]) sum([for (j = [0:i]) j])]);
x = 10; y = x * 2; echo(x, y, [x, y]);
echo([for (i = 0, s = 0; i < 4; i = i + 1, s = s + i) s]);
"#;

fn our_echoes() -> Vec<String> {
    let out = common::run(SRC);
    out.echoes.iter().map(|e| format!("ECHO: {}", e.text)).collect()
}

#[test]
fn echo_output_matches_openscad() {
    let ours = our_echoes();
    assert!(ours.len() > 30);
    let dir = env!("CARGO_TARGET_TMPDIR");
    let scad = format!("{dir}/compat.scad");
    let echo = format!("{dir}/compat.echo");
    std::fs::write(&scad, SRC).unwrap();
    let Ok(st) = Command::new("openscad").args(["-o", &echo, &scad]).output() else {
        eprintln!("openscad not installed, skipping comparison");
        return;
    };
    assert!(st.status.success(), "openscad failed: {}", String::from_utf8_lossy(&st.stderr));
    let theirs: Vec<String> =
        std::fs::read_to_string(&echo).unwrap().lines().filter(|l| l.starts_with("ECHO:")).map(String::from).collect();
    for (i, (a, b)) in ours.iter().zip(theirs.iter()).enumerate() {
        assert_eq!(a, b, "echo line {i} differs");
    }
    assert_eq!(ours.len(), theirs.len());
}
