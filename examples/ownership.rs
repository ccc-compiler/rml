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

fn append_bang(s: String) -> String {
    rml_concat(s, &(String::from("!")))
}

fn incr(r: &mut i64) {
    { *r = ((*r) + 1i64); }
}

fn rml_user_main(_u0: ()) {
    { let a = String::from("hello"); { let b = append_bang(a); { rml_print_endline(&(b)); { let mut counter = 0i64; { incr((&mut counter)); { incr((&mut counter)); { rml_print_int(counter); { rml_print_newline(()); { let c = rml_clone(&(b)); { rml_print_endline(&(c)); rml_print_endline(&(b)) } } } } } } } } } }
}

fn main() {
    // 以大堆疊執行緒跑使用者程式，深遞迴 / 長鏈結串列的遞迴 drop 才不會爆堆疊
    let h = std::thread::Builder::new().stack_size(1 << 30).spawn(|| rml_user_main(())).unwrap();
    h.join().unwrap();
}
