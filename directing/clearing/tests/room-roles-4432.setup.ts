// #4432 — the room's roles come from chorus-api at run time (room-roles.ts).
// Suites that don't stand up a poller get the room as it was before Abby, so
// their expectations about wren/silas/kade still read true. room-roles-4432
// tests the list itself, including a fourth role and an empty room.
import { setRoomRoles } from '../src/room-roles';
setRoomRoles(['wren', 'silas', 'kade']);
