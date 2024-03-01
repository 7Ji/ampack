pub(crate) fn try_into_human_readble<N: Into<u64>>(original: N) -> (f64, char) {
    let mut number = original.into() as f64;
    const SUFFIXES: [char; 8] = ['B', 'K', 'M', 'G', 'T', 'P', 'E', 'Z' ];
    let mut suffix_id = 0;
    while number >= 1024.0 && suffix_id < 8 {
        number /= 1024.0;
        suffix_id += 1;
    }
    if suffix_id >= 8 {
        return (f64::NAN, '-')
    }
    (number, SUFFIXES[suffix_id])
}
