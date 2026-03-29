"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.DEFAULT_CONFIG = exports.VALID_ROLES = void 0;
exports.isValidRole = isValidRole;
exports.nowISO = nowISO;
const path_1 = require("path");
const MESSAGES_DIR = (0, path_1.resolve)(__dirname, '../..');
const CASCADEPROJECTS = (0, path_1.resolve)(MESSAGES_DIR, '..');
exports.VALID_ROLES = ['silas', 'kade', 'wren', 'jeff'];
exports.DEFAULT_CONFIG = {
    activeDir: (0, path_1.resolve)(MESSAGES_DIR, 'workflows/active'),
    archiveDir: (0, path_1.resolve)(MESSAGES_DIR, 'workflows/archive'),
    briefDirs: {
        silas: (0, path_1.resolve)(CASCADEPROJECTS, 'architect/briefs'),
        kade: (0, path_1.resolve)(CASCADEPROJECTS, 'engineer/briefs'),
        wren: (0, path_1.resolve)(CASCADEPROJECTS, 'product-manager/briefs'),
        jeff: (0, path_1.resolve)(CASCADEPROJECTS, 'product-manager/briefs'),
    },
    handoffLogPath: (0, path_1.resolve)(MESSAGES_DIR, 'logs/handoffs.log'),
};
function isValidRole(role) {
    return exports.VALID_ROLES.includes(role);
}
function nowISO() {
    return new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
}
//# sourceMappingURL=config.js.map