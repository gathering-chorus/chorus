// @test-type: unit — module state only; no live services
// @domain: messages
// @card: #4432
// @owner: wren
/** #4432 — Abby joins the room the way the other roles did: from the roles door's rows. */
import { setRoomRoles, roomRoles } from '../src/room-roles';
import { pickJeffMessageTargets } from '../src/server';
import { isRoleName } from '../src/router';

afterEach(() => setRoomRoles(['wren', 'silas', 'kade']));

test('a fourth agent role is in the room: an unaddressed message reaches her too', () => {
  setRoomRoles(['kade', 'silas', 'wren', 'abby-normal']);
  expect(pickJeffMessageTargets('morning').sort()).toEqual(['abby-normal', 'kade', 'silas', 'wren']);
});

test('@abby-normal targets only her, and she reads as a role, not a guest', () => {
  setRoomRoles(['kade', 'silas', 'wren', 'abby-normal']);
  expect(pickJeffMessageTargets('@abby-normal hi')).toEqual(['abby-normal']);
  expect(isRoleName('abby-normal')).toBe(true);
});

test('NEGATIVE PROOF: before the API answers the room is empty, never three guessed names', () => {
  setRoomRoles([]);
  expect(roomRoles()).toEqual([]);
  expect(pickJeffMessageTargets('morning')).toEqual([]);
  expect(isRoleName('wren')).toBe(false);
});

test('Jeff is a person, never a room role', () => {
  setRoomRoles(['jeff', 'wren']);
  expect(roomRoles()).toEqual(['wren']);
});
