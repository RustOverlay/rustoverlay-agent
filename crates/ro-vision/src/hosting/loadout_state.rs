use std::time::SystemTime;

use crate::hud::{InventoryReading, VitalsReading};

type Listener = Box<dyn Fn(&LoadoutState) + Send + Sync>;

/// The most recent thing the agent saw on screen.
///
/// State, not events: a loadout is a snapshot where only the latest matters, so modelling it as
/// events would mean a queue of stale inventories. Listeners run on the caller of [`Self::set`] —
/// the capture thread — so a listener touching UI state must marshal.
#[derive(Default)]
pub struct LoadoutState {
    inventory: Option<InventoryReading>,
    held_slot: Option<usize>,
    vitals: Option<VitalsReading>,
    inventory_open: bool,
    read_at: Option<SystemTime>,
    last_open_inventory: Option<InventoryReading>,
    hidden_since: Option<SystemTime>,
    listeners: Vec<Listener>,
}

impl LoadoutState {
    pub fn new() -> Self {
        Self::default()
    }

    /// None until the first frame has been read.
    pub fn inventory(&self) -> Option<&InventoryReading> {
        self.inventory.as_ref()
    }

    /// None when no belt slot is highlighted, or the belt is too uniformly blue to tell.
    pub fn held_slot(&self) -> Option<usize> {
        self.held_slot
    }

    pub fn vitals(&self) -> Option<&VitalsReading> {
        self.vitals.as_ref()
    }

    /// Whether the last frame had the inventory panel drawn.
    pub fn inventory_open(&self) -> bool {
        self.inventory_open
    }

    /// When the last frame was read, for a UI that wants to show staleness.
    pub fn read_at(&self) -> Option<SystemTime> {
        self.read_at
    }

    /// The main grid, wear row and backpack as they were when the panel was last drawn, which
    /// outlives the panel being shut. None until it has been open once.
    ///
    /// Shutting the inventory hides those containers; it does not empty them. Dropping the reading
    /// with the panel published an empty inventory a second after a correct one. Retained, not
    /// trusted: a reload draws ammo out of the main grid and dying empties everything, so callers
    /// must present these against [`Self::hidden_since`] rather than as current.
    pub fn last_open_inventory(&self) -> Option<&InventoryReading> {
        self.last_open_inventory.as_ref()
    }

    /// When the panel was first seen shut after `last_open_inventory` was read. None while open.
    pub fn hidden_since(&self) -> Option<SystemTime> {
        self.hidden_since
    }

    /// Registers a listener run after every [`Self::set`], on the thread that called it.
    pub fn subscribe(&mut self, listener: impl Fn(&LoadoutState) + Send + Sync + 'static) {
        self.listeners.push(Box::new(listener));
    }

    /// Records one frame's reading and then notifies every listener.
    pub fn set(
        &mut self,
        inventory: InventoryReading,
        held_slot: Option<usize>,
        vitals: Option<VitalsReading>,
        inventory_open: bool,
        read_at: SystemTime,
    ) {
        if inventory_open {
            self.last_open_inventory = Some(inventory.clone());
            self.hidden_since = None;
        } else if self.last_open_inventory.is_some() {
            self.hidden_since.get_or_insert(read_at);
        }

        self.inventory = Some(inventory);
        self.held_slot = held_slot;
        self.vitals = vitals;
        self.inventory_open = inventory_open;
        self.read_at = Some(read_at);

        for listener in &self.listeners {
            listener(self);
        }
    }
}
