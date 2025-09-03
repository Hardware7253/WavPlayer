// Define a bytes type which is an array of bytes
use heapless::String;

pub type Bytes<const L: usize> = [u8; L];
use crate::{rprint, rprintln};

pub trait BytesTrait {
    fn get_bytes_section<const N: usize>(&self, start_byte: usize) -> Bytes<N>;
    fn slice_by<const N: usize, const T: usize>(&self) -> [Bytes<T>; N];
    fn print_bytes(&self);
    fn decode_ascii<const N: usize>(&self, skip_bytes: usize) -> String<N>;
}

impl<const L: usize> BytesTrait for Bytes<L> {

    // Returns a new bytes array which is a section of the original
    // Starts at start_byte and ends at start_bye + N - 1 (so there are N bytes in the array)
    fn get_bytes_section<const N: usize>(&self, start_byte: usize) -> Bytes<N> {
        let mut output = [0_u8; N];

        for (i, byte) in self.iter().skip(start_byte).enumerate() {
            if i == N {
                break;
            }

            output[i] = *byte;
        }

        output
    }

    // Splits the original byte array into N byte arrays with length T
    fn slice_by<const N: usize, const T: usize>(&self) -> [Bytes<T>; N] {
        let slices = N;
        let slice_len = T;

        let mut output = [[0_u8; T]; N];

        for slice_no in 0..slices {
            for byte_no in 0..slice_len {
                let this_byte_index = slice_len * slice_no + byte_no;
                output[slice_no][byte_no] = self[this_byte_index];
            }
        }

        output
    }

    // Prints the bytes array in hex
    fn print_bytes(&self) {

        for byte in self {
            rprint!("{:X} ", byte);
        }
        rprintln!();
    }

    // Decodes an ascii array of bytes
    // Only decodes N bytes into the string
    // Skips skip_bytes number of bytes
    fn decode_ascii<const N: usize>(&self, skip_bytes: usize) -> String<N> {
        String::from_iter(self.iter().skip(skip_bytes).take(N).map(|x| *x as char))
    }
}