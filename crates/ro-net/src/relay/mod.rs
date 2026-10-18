//! The relay session: one WebSocket per active team, binary protobuf frames both ways.

mod active_team_store;
mod combat_mapper;
mod loadout_mapper;
mod proxy_resolver;
mod proxy_tunnel;
mod relay_connection;
mod relay_options;
mod relay_uplink;
mod session_changes;
mod team_room_state;
mod wire_conversion;

pub use active_team_store::{ActiveTeam, ActiveTeamStore};
pub use combat_mapper::combat_to_proto;
pub use loadout_mapper::{loadout_to_proto, vitals_to_proto};
pub use proxy_resolver::HttpProxy;
pub use proxy_tunnel::{RelaySocket, connect};
pub use relay_connection::{RelayConnection, RelayError};
pub use relay_options::RelayOptions;
pub use relay_uplink::{RelayUplink, refusal};
pub use team_room_state::{TeamMemberView, TeamRoomState};
