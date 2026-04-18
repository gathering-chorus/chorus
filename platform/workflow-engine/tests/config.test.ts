import { VALID_ROLES, isValidRole, nowISO, DEFAULT_CONFIG } from '../src/config';

describe('config', () => {
  describe('VALID_ROLES', () => {
    it('contains exactly 4 roles', () => {
      expect(VALID_ROLES).toHaveLength(4);
    });

    it('includes silas, kade, wren, jeff', () => {
      expect(VALID_ROLES).toContain('silas');
      expect(VALID_ROLES).toContain('kade');
      expect(VALID_ROLES).toContain('wren');
      expect(VALID_ROLES).toContain('jeff');
    });
  });

  describe('isValidRole', () => {
    it('returns true for valid roles', () => {
      expect(isValidRole('silas')).toBe(true);
      expect(isValidRole('kade')).toBe(true);
      expect(isValidRole('wren')).toBe(true);
      expect(isValidRole('jeff')).toBe(true);
    });

    it('returns false for invalid roles', () => {
      expect(isValidRole('bob')).toBe(false);
      expect(isValidRole('')).toBe(false);
      expect(isValidRole('SILAS')).toBe(false);
    });
  });

  describe('nowISO', () => {
    it('returns ISO 8601 format', () => {
      const ts = nowISO();
      expect(ts).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
    });

    it('returns UTC timezone', () => {
      const ts = nowISO();
      expect(ts.endsWith('Z')).toBe(true);
    });
  });

  describe('DEFAULT_CONFIG', () => {
    it('has activeDir path', () => {
      expect(DEFAULT_CONFIG.activeDir).toBeTruthy();
      expect(DEFAULT_CONFIG.activeDir).toContain('workflows/active');
    });

    it('has archiveDir path', () => {
      expect(DEFAULT_CONFIG.archiveDir).toBeTruthy();
      expect(DEFAULT_CONFIG.archiveDir).toContain('workflows/archive');
    });

    it('has brief dirs for all roles', () => {
      expect(DEFAULT_CONFIG.briefDirs['silas']).toContain('roles/silas/briefs');
      expect(DEFAULT_CONFIG.briefDirs['kade']).toContain('roles/kade/briefs');
      expect(DEFAULT_CONFIG.briefDirs['wren']).toContain('roles/wren/briefs');
      expect(DEFAULT_CONFIG.briefDirs['jeff']).toContain('roles/wren/briefs');
    });
  });
});
