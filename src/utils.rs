use num::{BigUint, One, Zero};

pub fn big_binomial(n: usize, k: usize) -> BigUint {
    if k > n {
        BigUint::zero()
    } else {
        let k = k.min(n - k);
        let mut acc = BigUint::one();
        for (factor, dividend) in (n - k + 1..=n).zip(1..=k) {
            acc *= factor;
            acc /= dividend;
        }
        acc
    }
}

pub fn adjacent_mine_count_to_char(adjacent_mine_count: u8) -> char {
    match adjacent_mine_count {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        _ => unreachable!("adjacent mine count should never exceed exceed 8, yet is reported to be {adjacent_mine_count}"),
    }
}
