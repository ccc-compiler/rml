#![allow(warnings)]
// ── RML runtime：極薄的內建函式，全部會被 inline ──
#[inline(always)] fn rml_print_int(x: i64) { print!("{}", x); }
#[inline(always)] fn rml_print_float(x: f64) { print!("{:?}", x); }
#[inline(always)] fn rml_print_string(s: &String) { print!("{}", s); }
#[inline(always)] fn rml_print_endline(s: &String) { println!("{}", s); }
#[inline(always)] fn rml_print_newline(_u: ()) { println!(); }
#[inline(always)] fn rml_string_of_int(x: i64) -> String { x.to_string() }
#[inline(always)] fn rml_string_of_float(x: f64) -> String { format!("{:?}", x) }
#[inline(always)] fn rml_string_length(s: &String) -> i64 { s.len() as i64 }
#[inline(always)] fn rml_concat(mut a: String, b: &String) -> String { a.push_str(b); a }
#[inline(always)] fn rml_float_of_int(x: i64) -> f64 { x as f64 }
#[inline(always)] fn rml_int_of_float(x: f64) -> i64 { x as i64 }
#[inline(always)] fn rml_sqrt(x: f64) -> f64 { x.sqrt() }
#[inline(always)] fn rml_abs(x: i64) -> i64 { x.abs() }
#[inline(always)] fn rml_min(a: i64, b: i64) -> i64 { if a < b { a } else { b } }
#[inline(always)] fn rml_max(a: i64, b: i64) -> i64 { if a > b { a } else { b } }
#[inline(always)] fn rml_clone<T: Clone>(x: &T) -> T { x.clone() }


#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum List<Ta> {
    Nil,
    Cons(Ta, Box<List<Ta>>),
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Copy)]
enum Option_<Ta> {
    None,
    Some(Ta),
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum Tree<Ta> {
    Leaf,
    Node(Box<Tree<Ta>>, Ta, Box<Tree<Ta>>),
}

fn insert(t: Tree<i64>, x: i64) -> Tree<i64> {
    match t {
        Tree::Leaf => { Tree::Node(Box::new(Tree::Leaf), x, Box::new(Tree::Leaf)) },
        Tree::Node(l, v, r) => { if (x < v) { Tree::Node(Box::new(insert((*l), x)), v, r) } else { if (x > v) { Tree::Node(l, v, Box::new(insert((*r), x))) } else { Tree::Node(l, v, r) } } }
    }
}

fn size(t: &Tree<i64>) -> i64 {
    match t {
        Tree::Leaf => { 0i64 },
        Tree::Node(l, _, r) => { ((size((&**l)) + 1i64) + size((&**r))) }
    }
}

fn sum(t: &Tree<i64>) -> i64 {
    match t {
        Tree::Leaf => { 0i64 },
        Tree::Node(l, v, r) => { ((sum((&**l)) + (*v)) + sum((&**r))) }
    }
}

fn build(mut i: i64, mut n: i64, mut t: Tree<i64>) -> Tree<i64> {
    loop { return if (i == n) { t } else { { let __t0 = (i + 1i64); let __t1 = n; let __t2 = insert(t, ((i * 7919i64) % 100003i64)); i = __t0; n = __t1; t = __t2; continue; } }; }
}

fn rml_user_main(_u0: ()) {
    { let t = build(0i64, 200000i64, Tree::Leaf); { rml_print_int(size((&t))); { rml_print_newline(()); { rml_print_int(sum((&t))); rml_print_newline(()) } } } }
}

fn main() {
    // 以大堆疊執行緒跑使用者程式，深遞迴 / 長鏈結串列的遞迴 drop 才不會爆堆疊
    let h = std::thread::Builder::new().stack_size(1 << 30).spawn(|| rml_user_main(())).unwrap();
    h.join().unwrap();
}
