// Module for small helper functions

// Converts milliseconds to clock cycles
pub fn ms_to_cycles(millis: u64, clock_mhz: u64) -> u64 {
    millis * clock_mhz * 1000
}

// Adds a number to another number, min and max inclusive
// If the sum is greater than the max value, rollover
// Return calculated value or original sum
// E.g. add_with_rollover(5, 7, 0, 10) -> 2
pub fn add_with_rollover<T>(init: T, add: T, min: T, max: T) -> T
where
    T: core::ops::Add<Output = T> + core::ops::Sub<Output = T> + PartialOrd,
{
    let sum = init + add;

    if sum > max {
        return min + sum - max;
    }
    return sum;
}