use std::io;

fn f(n: i32) {
    let mut a: i32 = 1;
    let mut b: i32 = 1;
    match n {
        1 => print!("1"),
        2 => print!("1 1 "),
        _ => {
            print!("1 1 ");
            for _ in 0..n - 2 {
                print!("{} ", a + b);
                let tmp = a;
                a += b;
                b = tmp;
            }
        }
    }
}

fn main() {
    let mut num_str = String::new();
    io::stdin().read_line(&mut num_str).unwrap();
    let num: i32 = num_str.trim().parse().unwrap();
    println!("斐波那契数列前 {num} 项");
    f(num);
}
