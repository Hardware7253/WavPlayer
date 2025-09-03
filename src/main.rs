#![no_std]
#![no_main]

use cortex_m_rt::entry;

use rtt_target::{rprint, rprintln, rtt_init_print, ChannelMode};

use stm32f4xx_hal::{
    pac,
    prelude::*,

    gpio::NoPin,
    i2s::{I2s, stm32_i2s_v12x},

    sdio::{ClockFreq, SdCard, Sdio},
};

use stm32_i2s_v12x::{transfer::*, driver::{I2sDriver, I2sDriverConfig, DataFormat}};
use stm32f4xx_hal::dma::{StreamsTuple, Transfer, config::DmaConfig, StreamX, MemoryToPeripheral};

use core::cell::RefCell;
use cortex_m::interrupt::Mutex;
use stm32f4xx_hal::pac::interrupt;

// Test buffer
// const SINE_375_U16_STEREO: [u16; 256] = [
//     0, 0, 1607, 1607, 3211, 3211, 4807, 4807, 6392, 6392, 7961, 7961, 9511, 9511, 11038, 11038,
//     12539, 12539, 14009, 14009, 15446, 15446, 16845, 16845, 18204, 18204, 19519, 19519, 20787, 20787,
//     22004, 22004, 23169, 23169, 24278, 24278, 25329, 25329, 26318, 26318, 27244, 27244, 28105, 28105,
//     28897, 28897, 29621, 29621, 30272, 30272, 30851, 30851, 31356, 31356, 31785, 31785, 32137, 32137,
//     32412, 32412, 32609, 32609, 32727, 32727, 32767, 32767, 32727, 32727, 32609, 32609, 32412, 32412,
//     32137, 32137, 31785, 31785, 31356, 31356, 30851, 30851, 30272, 30272, 29621, 29621, 28897, 28897,
//     28105, 28105, 27244, 27244, 26318, 26318, 25329, 25329, 24278, 24278, 23169, 23169, 22004, 22004,
//     20787, 20787, 19519, 19519, 18204, 18204, 16845, 16845, 15446, 15446, 14009, 14009, 12539, 12539,
//     11038, 11038, 9511, 9511, 7961, 7961, 6392, 6392, 4807, 4807, 3211, 3211, 1607, 1607, 0, 0,
//     63929, 63929, 62325, 62325, 60729, 60729, 59144, 59144, 57575, 57575, 56025, 56025, 54598, 54598,
//     53097, 53097, 51627, 51627, 50190, 50190, 48791, 48791, 47432, 47432, 46117, 46117, 44849, 44849,
//     43632, 43632, 42467, 42467, 41358, 41358, 40307, 40307, 39318, 39318, 38392, 38392, 37531, 37531,
//     36739, 36739, 36015, 36015, 35361, 35361, 34782, 34782, 34277, 34277, 33848, 33848, 33496, 33496,
//     33221, 33221, 33024, 33024, 32906, 32906, 32866, 32866, 32906, 32906, 33024, 33024, 33221, 33221,
//     33496, 33496, 33848, 33848, 34277, 34277, 34782, 34782, 35361, 35361, 36015, 36015, 36739, 36739,
//     37531, 37531, 38392, 38392, 39318, 39318, 40307, 40307, 41358, 41358, 42467, 42467, 43632, 43632,
//     44849, 44849, 46117, 46117, 47432, 47432, 48791, 48791, 50190, 50190, 51627, 51627, 53097, 53097,
//     54598, 54598, 56025, 56025, 57575, 57575, 59144, 59144, 60729, 60729, 62325, 62325, 63929, 63929,
// ];


// Block size of the sd card
// This is also used as the exfat sector size
// These parameters typically correspond, otherwise the card will need to be reformatted
pub const BLOCK_SIZE: usize = 512;

const BUF_BLOCKS: usize = 1;
const BUF_SIZE: usize = BLOCK_SIZE * BUF_BLOCKS / 2;

type I2sDma = Transfer<StreamX<pac::DMA1, 4>, 0, I2sDriver<I2s<pac::SPI2>, Master, Transmit, Philips>, MemoryToPeripheral, &'static [u16; BUF_SIZE]>;
static G_TRANSFER: Mutex<RefCell<Option<I2sDma>>> = Mutex::new(RefCell::new(None));


const SAMPLE_RATE: u32 = 44_100;

pub mod block_device;
pub mod exfat;
pub mod bytes;
pub mod binary_helpers;
pub mod riff;
pub mod wav;
pub mod audio_buffer;
use audio_buffer::*;

const SILENCE_BUFFER: [u16; BUF_SIZE] = [0; BUF_SIZE];
static mut G_DBUF: [[u16; BUF_SIZE]; 2] = [[0; BUF_SIZE]; 2];
static G_DBUF_INFO: Mutex<RefCell<Option<DbufInfo>>> = Mutex::new(RefCell::new(Some(DbufInfo { 
    buf_states: [AudioBufState::Playing, AudioBufState::Empty], 
}))); 

// Implement block device trait for the sd card
impl block_device::BlockDevice<512> for Sdio<SdCard> {
    fn read_to_block(&mut self, blockaddr: u32, block: &mut [u8; BLOCK_SIZE]) -> Result<(), ()> {

        // rprintln!("Bout to read to block: {:X}", blockaddr);
        match self.read_block(blockaddr, block) {
            Ok(()) => return Ok(()),
            Err(_err) => {
                return Err(())
            }
        }
    }
}


#[entry]
fn main() -> ! {
    rtt_init_print!(ChannelMode::BlockIfFull, 4096);
    let cp = cortex_m::Peripherals::take().unwrap(); // Core peripherals
    let dp = pac::Peripherals::take().unwrap(); // Device peripherals

    let gpiob = dp.GPIOB.split();
    let gpioc = dp.GPIOC.split();
    let gpiod = dp.GPIOD.split();

    let rcc = dp.RCC.constrain();

    // Use cube ide to find clock combinations
    let clocks = rcc
        .cfgr
        .use_hse(8.MHz())
        .sysclk(96.MHz())
        .require_pll48clk()
        .i2s_clk(96.MHz()) // 44.1KHz
        .freeze();

    assert!(clocks.is_pll48clk_valid());

    let mut delay = cp.SYST.delay(&clocks);

    // Enable interrupt
    unsafe {
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::DMA1_STREAM4); // Enable interrupt for i2s dma
    }

    // Setup ip i2s peripheral 
    let i2s_pins = (gpiob.pb12, gpiob.pb10, NoPin::new(), gpioc.pc3); // WS, CK, SD
    let i2s = I2s::new(dp.SPI2, i2s_pins, &clocks);
    let i2s_config = I2sDriverConfig::new_master()
        .transmit()
        .standard(Philips)
        .data_format(DataFormat::Data16Channel16)
        .request_frequency(SAMPLE_RATE);

    let mut i2s_driver = I2sDriver::new(
        i2s,
        i2s_config
    );
    i2s_driver.enable();
    i2s_driver.set_tx_dma(true);

    rprintln!("Actual sample rate is {}", i2s_driver.sample_rate());

    // Set up SDIO interface
    let d0 = gpioc.pc8.internal_pull_up(true);
    let d1 = gpioc.pc9.internal_pull_up(true);
    let d2 = gpioc.pc10.internal_pull_up(true);
    let d3 = gpioc.pc11.internal_pull_up(true);
    let clk = gpioc.pc12;
    let cmd = gpiod.pd2.internal_pull_up(true);
    let mut sdio: Sdio<SdCard> = Sdio::new(dp.SDIO, (clk, cmd, d0, d1, d2, d3), &clocks);

    // Wait for card to be ready
    loop {
        match sdio.init(ClockFreq::F4Mhz) {
            Ok(_) => break,
            Err(err) => rprintln!("{:?}", err),
        }

        delay.delay_ms(1000);
    }

    let nblocks = sdio.card().map(|c| c.block_count()).unwrap_or(0);
    rprintln!("Card detected: nbr of blocks: {:?}", nblocks);

    let mut exfat = exfat::ExFat::new(sdio).unwrap();

    // List root directory
    let dir = exfat.list_directory(exfat.first_cluster_of_root_directory).unwrap();
    for (i, fs_entry) in dir.iter().enumerate() {
        rprintln!("entry {}: {:?}", i, &fs_entry);
    }

    // Open wav file
    let file_indx = 11;
    let wav_file = wav::WavFile::new(&mut exfat, &dir[file_indx]);
    rprintln!("{:?}", wav_file);

    let mut wav_file = wav_file.unwrap();

    let steams = StreamsTuple::new(dp.DMA1);
    let stream = steams.4;

    let mut transfer = unsafe {
        I2sDma::init_memory_to_peripheral(
            stream, 
            i2s_driver, 
            &G_DBUF[0],
            Some(&G_DBUF[1]),
            DmaConfig::default()
            .memory_increment(true)
            .double_buffer(true)
            .fifo_error_interrupt(true)
            .transfer_complete_interrupt(true)
        )
    };
    transfer.clear_all_flags();

    cortex_m::interrupt::free(|cs| {
        G_TRANSFER.borrow(cs).replace(Some(transfer));
        G_TRANSFER.borrow(cs).borrow_mut().as_mut().unwrap().start(|_tx| {});
    });


    let mut wav_bytes = [0u8; BLOCK_SIZE];
    'main: loop {
        // Find buffer to fill
        let mut fill_indx: Option<usize> = None;
        cortex_m::interrupt::free(|cs| {
            let dbuf_info_ref = G_DBUF_INFO.borrow(cs).borrow();
            let dbuf_info = dbuf_info_ref.as_ref().unwrap();

            fill_indx = dbuf_info.find_buffer(AudioBufState::Empty);
        });

        if let Some(fill_indx) = fill_indx {
            let buf = unsafe {&mut G_DBUF[fill_indx]};

            // Update this buf state to Filling
            cortex_m::interrupt::free(|cs| {
                G_DBUF_INFO.borrow(cs).borrow_mut().as_mut().unwrap().buf_states[fill_indx] = AudioBufState::Filling;
            });

            // This for loop fills the i2s buffer with multiple blocks of PCM data
            let mut buf_indx = 0;
            for _ in 0..BUF_BLOCKS { 

                // Get raw PCM bytes from wav file
                match wav_file.get_next_pcm_block(&mut exfat, &mut wav_bytes) {
                    Err(_) => {
                        rprintln!("Error, {}", wav_file.bytes_read);
                        continue 'main;
                    },
                    Ok(_) => (),
                };

                // Fill buf
                for (i, num) in wav_bytes.iter().enumerate().step_by(2) {
                    let sample = u16::from_le_bytes([*num, wav_bytes[i + 1]]);
                    
                    buf[buf_indx] = sample;
                    // buf[buf_indx] = SINE_375_U16_STEREO[buf_indx % SINE_375_U16_STEREO.len()]; // Fill with const buf instead
                    buf_indx += 1;
                }
            }

            // Update this buf state to Fillied
            cortex_m::interrupt::free(|cs| {
                let mut dbuf_info_ref = G_DBUF_INFO.borrow(cs).borrow_mut();
                let buf_state = &mut dbuf_info_ref.as_mut().unwrap().buf_states[fill_indx];
                *buf_state = AudioBufState::Filled;
            });

        }
    }
}

#[interrupt]
fn DMA1_STREAM4() {
    cortex_m::interrupt::free(|cs| {
        if let Some(transfer) = G_TRANSFER.borrow(cs).borrow_mut().as_mut() {

            let mut dbuf_info_ref = G_DBUF_INFO.borrow(cs).borrow_mut();
            let dbuf_info = dbuf_info_ref.as_mut().unwrap();

            let play_indx = dbuf_info.find_buffer(AudioBufState::Filled);

            if let Some(play_indx) = play_indx {
                let next_buf_state = &mut dbuf_info.buf_states[play_indx];

                let next_buf = unsafe{&G_DBUF[play_indx]};
                let result = transfer.next_transfer(next_buf);
                match result {
                    Ok(_) => {

                        // Change buf states
                        *next_buf_state = AudioBufState::Playing;
                        let old_buf_state = &mut dbuf_info.buf_states[play_indx ^ 1];
                        *old_buf_state = AudioBufState::Empty;
                    },

                    Err(err) => (),
                }
            } else {
                if transfer.flags().is_transfer_complete() {
                    let _ = transfer.next_transfer(&SILENCE_BUFFER);
                }
            }

            transfer.clear_all_flags();

        }
    });
}

use core::panic::PanicInfo;
#[inline(never)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {

    rprintln!("{}", info);
    loop {
    } 
}