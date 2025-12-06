#[cfg(feature = "logtofile")]
use log::info;
use std::{
    collections::HashMap,
    sync::atomic::Ordering::Relaxed,
    time::{Duration, Instant},
};
use windows::Win32::Networking::WinSock::{SOCKADDR, SOCKET};

use crate::{
    input_to_accum, println, ptr_wrap, rollback::Rollbacker, INPUT_KEYS_NUMBERS, LIKELY_DESYNCED,
    SOKU_FRAMECOUNT, TARGET_OFFSET, WARNING_FRAME_MISSING_1_COUNTDOWN,
    WARNING_FRAME_MISSING_2_COUNTDOWN,
};

// ============================================================================
// NetworkPacket
// ============================================================================

#[derive(Clone, Debug)]
pub struct NetworkPacket {
    pub id: usize,
    pub desyncdetect: u8,
    pub delay: u8,
    pub max_rollback: u8,
    pub inputs: Vec<u16>,
    pub last_confirm: usize,
    pub sync: Option<i32>,
    pub initial_max_rollback: Option<u8>,
}

impl NetworkPacket {
    pub fn encode(&self) -> Box<[u8]> {
        let mut buf = [0u8; 400];
        buf[4..8].copy_from_slice(&self.id.to_le_bytes());
        buf[8] = self.desyncdetect;
        buf[9] = self.delay;
        buf[10] = self.max_rollback;
        buf[11] = self.inputs.len() as u8;

        for (i, input) in self.inputs.iter().enumerate() {
            buf[12 + i * 2..14 + i * 2].copy_from_slice(&input.to_le_bytes());
        }

        let mut offset = 12 + self.inputs.len() * 2;
        buf[offset..offset + 4].copy_from_slice(&self.last_confirm.to_le_bytes());
        offset += 4;
        buf[offset..offset + 4].copy_from_slice(&self.sync.unwrap_or(i32::MAX).to_le_bytes());
        offset += 4;

        if let Some(initial_max_rollback) = self.initial_max_rollback {
            buf[offset] = initial_max_rollback;
            offset += 1;
        }

        buf[..offset].to_vec().into_boxed_slice()
    }

    pub fn decode(d: &[u8]) -> Self {
        let id = usize::from_le_bytes(d[4..8].try_into().unwrap());
        let desyncdetect = d[8];
        let delay = d[9];
        let max_rollback = d[10];
        let input_count = d[11] as usize;

        let inputs: Vec<u16> = (0..input_count)
            .map(|i| u16::from_le_bytes(d[12 + i * 2..14 + i * 2].try_into().unwrap()))
            .collect();

        let mut offset = 12 + input_count * 2;
        let last_confirm = usize::from_le_bytes(d[offset..offset + 4].try_into().unwrap());
        offset += 4;

        let sync_raw = i32::from_le_bytes(d[offset..offset + 4].try_into().unwrap());
        let sync = (sync_raw != i32::MAX).then_some(sync_raw);
        offset += 4;

        let initial_max_rollback = (d.len() > offset).then(|| d[offset]);

        Self {
            id,
            desyncdetect,
            delay,
            max_rollback,
            inputs,
            last_confirm,
            sync,
            initial_max_rollback,
        }
    }
}

// ============================================================================
// FrameTimeData
// ============================================================================

#[derive(Clone, Debug)]
pub enum FrameTimeData {
    Empty,
    LocalFirst(Instant),
    RemoteFirst(Instant),
    Done(i32),
}

// ============================================================================
// Netcoder
// ============================================================================

pub struct Netcoder {
    last_opponent_confirm: usize,
    id: usize,

    opponent_inputs: Vec<Option<u16>>,
    last_opponent_input: usize,

    inputs: Vec<u16>,

    send_times: HashMap<usize, Instant>,
    recv_delays: HashMap<usize, Duration>,
    real_rollback_to_be_showed: usize,

    pub delay: usize,
    pub max_rollback: usize,
    pub display_stats: bool,
    pub last_opponent_delay: usize,
    pub initial_opponent_max_rollback: Option<usize>,
    pub initial_my_max_rollback: usize,

    past_frame_starts: Vec<FrameTimeData>,

    pub receiver: std::sync::mpsc::Receiver<(NetworkPacket, Instant)>,
    time_syncs: Vec<i32>,
    last_median_sync: i32,

    pub autodelay_enabled: Option<i8>,

    old_to_be_sent: Option<NetworkPacket>,
    old_input: [bool; INPUT_KEYS_NUMBERS],
}

impl Netcoder {
    pub fn new(
        receiver: std::sync::mpsc::Receiver<(NetworkPacket, Instant)>,
        my_max_rollback: u8,
    ) -> Self {
        Self {
            last_opponent_confirm: 0,
            inputs: Vec::new(),
            opponent_inputs: Vec::new(),
            send_times: HashMap::new(),
            recv_delays: HashMap::new(),
            real_rollback_to_be_showed: 0,
            last_opponent_delay: 0,
            last_opponent_input: 0,
            id: 0,
            delay: 0,
            max_rollback: 6,
            display_stats: false,
            initial_opponent_max_rollback: None,
            initial_my_max_rollback: my_max_rollback as usize,
            past_frame_starts: Vec::new(),
            receiver,
            time_syncs: Vec::new(),
            last_median_sync: 0,
            autodelay_enabled: None,
            old_to_be_sent: None,
            old_input: [false; INPUT_KEYS_NUMBERS],
        }
    }

    // ========================================================================
    // Helper: Update delay display in game memory
    // ========================================================================
    #[inline]
    unsafe fn update_delay_display(&self) {
        let netmanager = *(0x8986a0 as *const usize);
        // Host delay display
        *ptr_wrap!((netmanager + 0x80) as *mut u8) = self.delay as u8;
        // Client delay display
        *ptr_wrap!((netmanager + 0x81) as *mut u8) = self.delay as u8;
    }

    // ========================================================================
    // Helper: Check if local player is P1 (host)
    // ========================================================================
    #[inline]
    fn is_p1() -> bool {
        unsafe {
            let netmanager = *(0x8986a0 as *const usize);
            netmanager != 0 && *ptr_wrap!(netmanager as *const usize) == 0x858cac
        }
    }

    // ========================================================================
    // Helper: Ensure vector capacity for frame data
    // ========================================================================
    #[inline]
    fn ensure_frame_capacity(&mut self) {
        if self.past_frame_starts.len() <= self.id {
            self.past_frame_starts
                .resize(self.id + 1, FrameTimeData::Empty);
        }
    }

    // ========================================================================
    // Helper: Process frame timing data from received packet
    // ========================================================================
    fn process_frame_timing(&mut self, packet_id: usize, recv_time: Instant) {
        // Ensure capacity
        if self.past_frame_starts.len() <= packet_id {
            self.past_frame_starts
                .resize(packet_id + 1, FrameTimeData::Empty);
        }

        match &self.past_frame_starts[packet_id] {
            FrameTimeData::Empty => {
                self.past_frame_starts[packet_id] = FrameTimeData::RemoteFirst(recv_time);
            }
            FrameTimeData::LocalFirst(local_time) => {
                let duration = recv_time
                    .checked_duration_since(*local_time)
                    .unwrap_or_else(|| {
                        local_time
                            .checked_duration_since(recv_time)
                            .expect("one duration calculation must succeed")
                    });
                self.past_frame_starts[packet_id] = FrameTimeData::Done(duration.as_micros() as i32);
            }
            FrameTimeData::RemoteFirst(_) | FrameTimeData::Done(_) => {}
        }
    }

    // ========================================================================
    // Helper: Handle opponent's sync timing data
    // ========================================================================
    fn handle_sync_timing(&mut self, packet: &NetworkPacket) {
        let Some(remote_sync) = packet.sync else {
            return;
        };

        if remote_sync < 0 {
            TARGET_OFFSET.fetch_add(-remote_sync.max(-5000), Relaxed);
            return;
        }

        let sync_frame = packet.id.saturating_sub(packet.inputs.len());
        match self.past_frame_starts.get(sync_frame) {
            Some(FrameTimeData::Done(local)) => {
                let diff = *local - remote_sync;
                while packet.id > self.time_syncs.len() {
                    self.time_syncs.push(0);
                }
                self.time_syncs.push(diff);
            }
            Some(FrameTimeData::RemoteFirst(_)) => {
                TARGET_OFFSET.fetch_add(-200, Relaxed);
            }
            _ => {}
        }
    }

    // ========================================================================
    // Helper: Check for desync based on weather data
    // ========================================================================
    fn check_desync(&self, packet: &NetworkPacket, rollbacker: &Rollbacker) {
        let weather_remote = packet.desyncdetect;
        let weather_local = rollbacker.weathers.get(&packet.id.saturating_sub(20)).copied().unwrap_or(0);

        unsafe {
            LIKELY_DESYNCED = weather_remote != weather_local;
        }

        #[cfg(feature = "logtofile")]
        if weather_remote != weather_local {
            info!(
                "DESYNC: local: {}, remote: {}",
                weather_local, weather_remote
            );
        }
    }

    // ========================================================================
    // Helper: Negotiate max rollback with opponent
    // ========================================================================
    fn negotiate_max_rollback(&mut self, opponent_max_rollback: u8) {
        let opponent = opponent_max_rollback as usize;
        self.initial_opponent_max_rollback = Some(opponent);

        let min = opponent.min(self.initial_my_max_rollback);
        let max = opponent.max(self.initial_my_max_rollback);

        // Choose rollback closest to 6, or 6 if preferences span it
        self.max_rollback = if min < 6 && 6 < max {
            6
        } else if max <= 6 {
            max
        } else {
            min
        };
    }

    // ========================================================================
    // Helper: Convert u16 input bitmask to bool array
    // ========================================================================
    #[inline]
    fn input_from_bitmask(bitmask: u16) -> [bool; INPUT_KEYS_NUMBERS] {
        let mut input = [false; INPUT_KEYS_NUMBERS];
        for i in 0..INPUT_KEYS_NUMBERS {
            input[i] = (bitmask & (1 << i)) != 0;
        }
        input
    }

    // ========================================================================
    // Helper: Process inputs from received packet
    // ========================================================================
    fn process_packet_inputs(&mut self, packet: &NetworkPacket, rollbacker: &mut Rollbacker, recv_time: Instant) {
        // Extend opponent inputs vector if needed
        let latest = packet.id;
        if self.opponent_inputs.len() <= latest {
            self.opponent_inputs.resize(latest + 1, None);
        }

        self.last_opponent_input = self.last_opponent_input.max(packet.id);

        // Update receive delays for confirmed frames
        for frame in (self.last_opponent_confirm + 1)..=packet.last_confirm {
            if let Some(send_time) = self.send_times.get(&frame) {
                let delay = recv_time.saturating_duration_since(*send_time);
                self.recv_delays.insert(frame, delay);
            }
        }
        self.last_opponent_confirm = self.last_opponent_confirm.max(packet.last_confirm);

        // Process inputs from newest to oldest
        let mut frame = latest;
        for input_bitmask in &packet.inputs {
            if self.opponent_inputs[frame].is_none() {
                // Frame 0 is special - always use zero input to avoid crashes
                let input = if frame == 0 { 0 } else { *input_bitmask };
                self.opponent_inputs[frame] = Some(input);
                rollbacker.enemy_inputs.insert(Self::input_from_bitmask(input), frame);
            }

            if frame == 0 {
                break;
            }
            frame -= 1;
        }
    }

    // ========================================================================
    // Helper: Process all pending packets from receiver
    // ========================================================================
    fn process_pending_packets(&mut self, rollbacker: &mut Rollbacker) {
        let is_p1 = Self::is_p1();

        while let Ok((packet, recv_time)) = self.receiver.try_recv() {
            // Skip stale packets from previous rounds
            if packet.id > self.id + 20 {
                continue;
            }

            // Process new frame data
            if packet.id >= self.opponent_inputs.len() {
                // Client syncs max_rollback from host
                if !is_p1 {
                    self.max_rollback = packet.max_rollback as usize;
                }

                // Update opponent delay display
                if self.display_stats {
                    unsafe { crate::NEXT_DRAW_ENEMY_DELAY = Some(packet.delay as i32) };
                } else {
                    unsafe { crate::NEXT_DRAW_ENEMY_DELAY = None };
                }
                self.last_opponent_delay = packet.delay as usize;

                // Process timing and sync data
                self.process_frame_timing(packet.id, recv_time);
                self.handle_sync_timing(&packet);
                self.check_desync(&packet, rollbacker);
            }

            // Negotiate max rollback if initial value provided
            if let Some(opponent_max_rollback) = packet.initial_max_rollback {
                self.negotiate_max_rollback(opponent_max_rollback);
            }

            // Process inputs
            self.process_packet_inputs(&packet, rollbacker, recv_time);
        }
    }

    // ========================================================================
    // Helper: Calculate and refresh ping display
    // ========================================================================
    fn refresh_ping_display(&self) {
        if !self.display_stats || self.id <= 90 {
            return;
        }

        let now = Instant::now();
        let max_delay = ((self.id - 90)..self.id)
            .filter_map(|frame| {
                self.recv_delays.get(&frame).map(|d| d.as_millis()).or_else(|| {
                    self.send_times
                        .get(&frame)
                        .map(|t| now.saturating_duration_since(*t).as_millis())
                })
            })
            .max()
            .unwrap_or(0);

        unsafe {
            crate::NEXT_DRAW_PING = Some((max_delay / 2) as i32);
        }
    }

    // ========================================================================
    // Helper: Check if we should pause waiting for opponent data
    // ========================================================================
    fn check_should_pause(&mut self) -> bool {
        // Pause if we're too far ahead of confirmed frames
        if self.id > self.last_opponent_confirm + 30 {
            println!(
                "frame is missing: id: {}, confirm: {}",
                self.id, self.last_opponent_confirm
            );
            unsafe { WARNING_FRAME_MISSING_1_COUNTDOWN = 120 };
            self.refresh_ping_display();
            return true;
        }

        // Pause if we're too far ahead of received inputs
        let max_ahead = (self.max_rollback + self.delay.max(self.last_opponent_delay)).min(15);
        if self.id > self.last_opponent_input + max_ahead {
            println!(
                "frame is missing for reason 2: id: {}, confirm: {}",
                self.id, self.last_opponent_confirm
            );
            unsafe {
                WARNING_FRAME_MISSING_2_COUNTDOWN = 120;
                if self.display_stats {
                    self.refresh_ping_display();
                    self.real_rollback_to_be_showed = self
                        .real_rollback_to_be_showed
                        .max(self.id - self.last_opponent_input - 1 - self.delay);
                    crate::NEXT_DRAW_ROLLBACK = Some(self.real_rollback_to_be_showed as i32);
                }
            }
            return true;
        }

        false
    }

    // ========================================================================
    // Helper: Send cached packet when paused
    // ========================================================================
    fn send_cached_packet(&mut self) {
        if let Some(packet) = self.old_to_be_sent.as_mut() {
            packet.last_confirm = self.last_opponent_input.min(packet.id + 30);
            packet.max_rollback = self.max_rollback as u8;
            unsafe { send_packet(packet.encode()) };
        }
    }

    // ========================================================================
    // Helper: Prepare and record local inputs
    // ========================================================================
    fn prepare_local_inputs(
        &mut self,
        rollbacker: &mut Rollbacker,
        current_input: [bool; INPUT_KEYS_NUMBERS],
    ) {
        // Merge with accumulated inputs from paused frames
        for (i, &pressed) in current_input.iter().enumerate() {
            self.old_input[i] |= pressed;
        }
        let merged_input = self.old_input;
        self.old_input = [false; INPUT_KEYS_NUMBERS];

        let input_head = self.id;

        // Extend rollbacker's self_inputs
        while rollbacker.self_inputs.len() <= input_head {
            let idx = rollbacker.self_inputs.len();
            rollbacker.self_inputs.push(if idx == 0 {
                [false; INPUT_KEYS_NUMBERS]
            } else {
                merged_input
            });
        }

        // Extend our input history
        while self.inputs.len() <= input_head {
            let idx = self.inputs.len();
            self.inputs.push(input_to_accum(&if idx == 0 {
                [false; INPUT_KEYS_NUMBERS]
            } else {
                merged_input
            }));
        }
    }

    // ========================================================================
    // Helper: Build and send outgoing packet
    // ========================================================================
    fn build_and_send_packet(&mut self, rollbacker: &Rollbacker) {
        let input_range = self.last_opponent_confirm..=self.id;
        let mut inputs: Vec<u16> = self.inputs[input_range].to_vec();
        inputs.reverse();

        let sync = self
            .past_frame_starts
            .get(self.id.saturating_sub(30))
            .and_then(|f| match f {
                FrameTimeData::Done(x) => Some(*x),
                _ => None,
            });

        let packet = NetworkPacket {
            id: self.id,
            desyncdetect: rollbacker
                .weathers
                .get(&self.id.saturating_sub(20))
                .copied()
                .unwrap_or(0),
            delay: self.delay as u8,
            max_rollback: self.max_rollback as u8,
            inputs,
            last_confirm: self.last_opponent_input.min(self.id + 30),
            sync,
            initial_max_rollback: (self.id <= 120).then_some(self.initial_my_max_rollback as u8),
        };

        self.old_to_be_sent = Some(packet.clone());
        unsafe { send_packet(packet.encode()) };
        self.send_times.insert(self.id, Instant::now());
    }

    // ========================================================================
    // Helper: Update rollback statistics display
    // ========================================================================
    fn update_rollback_stats(&mut self, rollbacker: &Rollbacker) {
        if self.display_stats {
            self.real_rollback_to_be_showed = rollbacker
                .guessed
                .len()
                .max(self.real_rollback_to_be_showed);

            if self.id % 60 == 0 {
                unsafe {
                    crate::NEXT_DRAW_ROLLBACK = Some(self.real_rollback_to_be_showed as i32);
                }
                self.real_rollback_to_be_showed = 0;
            }
        } else {
            unsafe { crate::NEXT_DRAW_ROLLBACK = None };
            self.real_rollback_to_be_showed = 0;
        }
    }

    // ========================================================================
    // Helper: Calculate auto-delay at frame 100
    // ========================================================================
    fn calculate_auto_delay(&mut self) {
        let Some(bias) = self.autodelay_enabled else {
            return;
        };

        if self.id != 100 {
            return;
        }

        let delays: Vec<u128> = (30..70)
            .filter_map(|f| self.recv_delays.get(&f))
            .map(|d| d.as_micros())
            .collect();

        if delays.is_empty() {
            return;
        }

        let sum: u128 = delays.iter().sum();
        let avg = sum / delays.len() as u128;
        self.delay = (avg.div_ceil(1_000_000 / 30) as i8 - bias).clamp(0, 9) as usize;
        println!("avg: {}, auto delay: {}", avg, self.delay);
    }

    // ========================================================================
    // Helper: Calculate and apply time sync adjustments
    // ========================================================================
    fn apply_time_sync(&mut self) {
        const INTERVAL: usize = 50;

        // Calculate median sync every INTERVAL frames
        if self.id % INTERVAL == 0 && self.id > INTERVAL + 30 {
            let range = (self.id - 30 - INTERVAL)..(self.id - 30);
            if let Some(slice) = self.time_syncs.get(range) {
                if slice.len() == INTERVAL {
                    let mut sorted: Vec<i32> = slice.to_vec();
                    sorted.sort_unstable();

                    // Trimmed mean (exclude 3 lowest and 3 highest)
                    let sum: i32 = sorted[3..INTERVAL - 3].iter().sum();
                    self.last_median_sync = sum / (INTERVAL as i32 - 6);
                }
            }
        }

        // Apply sync adjustment based on magnitude
        let offset = match self.last_median_sync.abs() {
            x if x > 20000 => self.last_median_sync / 700,
            x if x > 10000 => self.last_median_sync / 1400,
            x if x > 2000 => self.last_median_sync / 2000,
            x if x > 500 => self.last_median_sync.clamp(-1, 1),
            _ => 0,
        };
        TARGET_OFFSET.fetch_add(offset, Relaxed);
    }

    // ========================================================================
    // Helper: Record local frame start time
    // ========================================================================
    fn record_frame_start(&mut self, start_time: Instant) {
        match &self.past_frame_starts[self.id] {
            FrameTimeData::Empty => {
                self.past_frame_starts[self.id] = FrameTimeData::LocalFirst(start_time);
            }
            FrameTimeData::RemoteFirst(remote_time) => {
                let diff = remote_time
                    .saturating_duration_since(start_time)
                    .as_micros() as i32;
                self.past_frame_starts[self.id] = FrameTimeData::Done(diff);
            }
            FrameTimeData::LocalFirst(_) => unreachable!("LocalFirst set twice"),
            FrameTimeData::Done(_) => {}
        }
    }

    // ========================================================================
    // Main: Process frame and send packet
    // ========================================================================
    /// Process received packets and send the current frame to opponent.
    /// Returns the number of frames to advance.
    pub fn process_and_send(
        &mut self,
        rollbacker: &mut Rollbacker,
        current_input: [bool; INPUT_KEYS_NUMBERS],
    ) -> u32 {
        let function_start_time = Instant::now();

        // Initialize frame data structures
        self.ensure_frame_capacity();

        // Update delay display in game memory
        unsafe { self.update_delay_display() };

        // Brief sleep to allow netcode to finish processing
        // (Soku locks netcode until frame start)
        std::thread::sleep(Duration::from_millis(1));

        // Process all pending packets
        self.process_pending_packets(rollbacker);

        // Update ping display periodically
        if self.display_stats && self.id % 60 == 0 {
            self.refresh_ping_display();
        } else if !self.display_stats {
            unsafe { crate::NEXT_DRAW_PING = None };
        }

        // Check if we need to pause waiting for opponent
        if self.check_should_pause() {
            self.send_cached_packet();
            return 0;
        }

        // Prepare and record our inputs
        self.prepare_local_inputs(rollbacker, current_input);

        // Build and send packet to opponent
        self.build_and_send_packet(rollbacker);

        // Start rollback processing
        let frames_to_advance = rollbacker.start();

        // Adjust for delay
        let frame_diff = self.id as i64 - unsafe { *SOKU_FRAMECOUNT } as i64;
        let frames_to_advance = if frame_diff < self.delay as i64 {
            frames_to_advance.saturating_sub(1)
        } else if frame_diff > self.delay as i64 {
            frames_to_advance + 1
        } else {
            frames_to_advance
        };

        // Update statistics displays
        self.update_rollback_stats(rollbacker);
        self.calculate_auto_delay();
        self.apply_time_sync();

        // Record when we started processing this frame
        self.record_frame_start(function_start_time);

        // Advance to next frame
        self.id += 1;
        frames_to_advance as u32
    }
}

// ============================================================================
// Packet Sending Functions
// ============================================================================

pub unsafe fn send_packet(mut data: Box<[u8]>) {
    data[0] = 0x6b;

    let netmanager = *(0x8986a0 as *const usize);
    let socket = netmanager + 0x3e4;

    let (to, player_id) = if *ptr_wrap!(netmanager as *const usize) == 0x858cac {
        // Host
        let it = (netmanager + 0x4c8) as *const usize;
        if *it == 0 {
            panic!("null iterator in send_packet (host)");
        }
        (*(it as *const *const SOCKADDR), 1u8)
    } else {
        // Client
        if *(netmanager as *const usize) != 0x858d14 {
            panic!("unexpected netmanager state in send_packet (client)");
        }
        ((netmanager + 0x47c) as *const SOCKADDR, 2u8)
    };

    data[1] = player_id;

    // Use Soku's sendto to work with mods that hook the import table
    let soku_sendto: unsafe extern "stdcall" fn(
        SOCKET,
        *const u8,
        i32,
        i32,
        *const SOCKADDR,
        i32,
    ) -> i32 = std::mem::transmute(0x0081f6c4);

    let result = soku_sendto(
        *ptr_wrap!(socket as *const SOCKET),
        data.as_ptr(),
        data.len() as i32,
        0,
        to,
        0x10,
    );

    if result == -1 {
        // Socket error - could add error handling here
    }
}

pub unsafe fn send_packet_untagged(data: Box<[u8]>) {
    let netmanager = *(0x8986a0 as *const usize);
    let socket = netmanager + 0x3e4;

    let to = if *(netmanager as *const usize) == 0x858cac {
        // Host
        let it = (netmanager + 0x4c8) as *const usize;
        if *it == 0 {
            panic!("null iterator in send_packet_untagged (host)");
        }
        *(it as *const *const SOCKADDR)
    } else {
        // Client
        if *(netmanager as *const usize) != 0x858d14 {
            panic!("unexpected netmanager state in send_packet_untagged (client)");
        }
        (netmanager + 0x47c) as *const SOCKADDR
    };

    let soku_sendto: unsafe extern "stdcall" fn(
        SOCKET,
        *const u8,
        i32,
        i32,
        *const SOCKADDR,
        i32,
    ) -> i32 = std::mem::transmute(0x0081f6c4);

    let result = soku_sendto(
        *ptr_wrap!(socket as *const SOCKET),
        data.as_ptr(),
        data.len() as i32,
        0,
        to,
        0x10,
    );

    if result == -1 {
        println!(
            "socket err: {:?}",
            windows::Win32::Networking::WinSock::WSAGetLastError()
        );
    }
}
