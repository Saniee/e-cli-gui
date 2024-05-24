use std::sync::mpsc::{Receiver, Sender};

pub struct GuiChannels {
    pub dl_count_channel: (Sender<u64>, Receiver<u64>),
    pub dl_status_channel: (Sender<bool>, Receiver<bool>),
    pub finished_status_channel: (Sender<bool>, Receiver<bool>),
}

impl Default for GuiChannels {
    fn default() -> Self {
        let dl_count_channel = std::sync::mpsc::channel();
        let dl_status_channel = std::sync::mpsc::channel();
        let finished_status_channel = std::sync::mpsc::channel();

        Self {
            dl_count_channel,
            dl_status_channel,
            finished_status_channel,
        }
    }
}
