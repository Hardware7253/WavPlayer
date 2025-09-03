// Return true if a specified bit is on in a number
pub fn bit_on(num: u64, bit: u8) -> bool {
    let mask = 1 << bit;
    (num & mask) != 0
}

// Converts a u32 number containing signed data into the proper i32 type
pub fn convert_to_signed(num: u32) -> i32 {
    if bit_on(num as u64, 31) {
        let num_strip_top = (num ^ (1 << 31)) as i32; // Original number without the top bit
        return (1 << 31) + num_strip_top;
    }
    num as i32
}