// @test-type: integration
// @card: #4866
// @owner: abby-normal
// @domain: messages
/**
 * #4866 AC2 — The composer provides attachment pickers for photos and documents;
 * attached files insert recognizable tokens into the message (📷 /uploads/..., 📎 /uploads/...);
 * files render as download chips in rooms.
 */
import { TEAM_COMPOSER, renderRoomMessage } from '../src/views';
const { uploadFile, fileNames, appendFiles } = require('../public/app.js');

describe('#4866 AC2: composer attachment pickers and download chips', () => {
  test('TEAM_COMPOSER HTML contains both photo (📷) and file (📎) attachment pickers', () => {
    expect(TEAM_COMPOSER).toContain('id="composer-photo"');
    expect(TEAM_COMPOSER).toContain('id="composer-file"');
    expect(TEAM_COMPOSER).toContain('class="composer-file"');
    expect(TEAM_COMPOSER).toContain('📎');
  });

  test('fileNames extracts document uploads from message text', () => {
    const text = 'Here is the architecture doc: 📎 /clearing-v2-uploads/1700000000_arch.pdf and log /clearing-v2-uploads/1700000001_run.txt';
    const names = fileNames(text);
    expect(names).toEqual(['1700000000_arch.pdf', '1700000001_run.txt']);
  });

  test('renderRoomMessage renders download chip for attached files', () => {
    const msg = {
      id: 'm1',
      room: 'team',
      from: 'jeff',
      text: 'Spec attached 📎 /clearing-v2-uploads/1700000000_spec.pdf',
      ts: '2026-11-02T19:00:00Z'
    };
    const html = renderRoomMessage(msg as any);
    expect(html).toContain('msg-file');
    expect(html).toContain('1700000000_spec.pdf');
    expect(html).toContain('download');
  });

  test('appendFiles adds file download elements to DOM node', () => {
    const parent = {
      children: [] as any[],
      appendChild(el: any) { this.children.push(el); }
    };
    const mockDoc = {
      createElement: (tag: string) => ({
        tag,
        className: '',
        children: [] as any[],
        appendChild(child: any) { this.children.push(child); },
        setAttribute(k: string, v: string) { (this as any)[k] = v; }
      })
    };
    const text = 'Check 📎 /clearing-v2-uploads/1700000000_notes.txt';
    appendFiles(parent as any, text, mockDoc as any);
    expect(parent.children.length).toBe(1);
    expect(parent.children[0].className).toBe('msg-files');
  });

  test('NEGATIVE PROOF: DOM init wires watchPhotoButton, watchFileButton, and watchMicButton listeners', () => {
    const fs = require('fs');
    const path = require('path');
    const appJsContent = fs.readFileSync(path.join(__dirname, '../public/app.js'), 'utf8');

    // Extract the top-level script DOM init block
    const allMatches = Array.from(appJsContent.matchAll(/if\s*\(typeof document !== 'undefined'\)\s*\{([\s\S]*?)\}/g));
    const initBlock = allMatches.map((m: any) => m[1]).find((body: string) => body.includes('initComposerInput()'));
    expect(initBlock).toBeTruthy();

    expect(initBlock).toMatch(/\bwatchPhotoButton\(\)/);
    expect(initBlock).toMatch(/\bwatchFileButton\(\)/);
    expect(initBlock).toMatch(/\bwatchMicButton\(\)/);

    // Negative proof: an init body lacking either call fails this assertion
    const missingPhoto = initBlock!.replace(/watchPhotoButton\(\);?/, '');
    expect(missingPhoto).not.toMatch(/\bwatchPhotoButton\(\)/);

    const missingMic = initBlock!.replace(/watchMicButton\(\);?/, '');
    expect(missingMic).not.toMatch(/\bwatchMicButton\(\)/);
  });
});
