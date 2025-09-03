// Define a block device trait for interfacing with the storage device 
pub trait BlockDevice<const L: usize> {
    fn read_to_block(&mut self, blockaddr: u32, block: &mut [u8; L]) -> Result<(), ()>;

    fn read_block(&mut self, blockaddr: u32) -> Result<[u8; L], ()> {
        let mut block = [0_u8; L];
        let result = self.read_to_block(blockaddr, &mut block);

        match result {
            Ok(()) => return Ok(block),
            Err(()) => return Err(())
        }
    }
}




