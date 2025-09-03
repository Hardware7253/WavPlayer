#[derive(PartialEq, Debug, Clone, Copy)]
pub enum AudioBufState {
    Filling,
    Filled,
    Playing,
    Empty,
}

#[derive(Debug)]
pub struct DbufInfo {
    pub buf_states: [AudioBufState; 2],
}

impl DbufInfo {

    // Finds the index of the first buffer with the state provided in the paramter
    pub fn find_buffer(&self, match_state: AudioBufState) -> Option<usize> {
        for (i, buf_state) in self.buf_states.iter().enumerate() {
            if *buf_state == match_state {
                return Some(i)
            }
        }
        return None;
    }
}

// Hello me try decoupling further by usings a playing and a fillind index
// Only have filling updated by cpu
// Only have playing updated by ISR