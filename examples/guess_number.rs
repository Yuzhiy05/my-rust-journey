use rand::RngExt;
use std::cmp;
use std::io;
fn main() {
    // 生成一个1-100的随机数
    let mut rng = rand::rng();
    let rand_num = rng.random::<u32>() % 100 + 1;

    println!("猜一个1-100的数字");

    loop {
        let mut str_num = String::new();
        io::stdin().read_line(&mut str_num).unwrap();
        let num: u32 = str_num.trim().parse().unwrap();
        match num.cmp(&rand_num) {
            cmp::Ordering::Greater => println!("太大了"),
            cmp::Ordering::Less => println!("太小了"),
            cmp::Ordering::Equal => {
                println!("正确!");
                break;
            }
        }
    }
}
