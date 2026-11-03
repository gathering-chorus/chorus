// @test-type: integration
// @card: #4866
// @owner: abby-normal
// @domain: messages
/**
 * #4866 AC2 — The composer provides a unified file/photo attachment picker (📎);
 * camera button (📷) and watchPhotoButton are retired.
 * Attached files insert recognizable tokens into the message (📎 /uploads/...);
 * photos render inline in rooms, while non-image files render as download chips.
 */
import { TEAM_COMPOSER, renderRoomMessage, renderRoomsPage } from '../src/views';
const { uploadFile, fileNames, photoNames, appendFiles } = require('../public/app.js');

describe('#4866 AC2: composer attachment picker and inline photos / download chips', () => {
  test('TEAM_COMPOSER and renderRoomsPage HTML contain unified file picker (📎) and retire camera (📷)', () => {
    // File picker present
    expect(TEAM_COMPOSER).toContain('id="composer-file"');
    expect(TEAM_COMPOSER).toContain('class="composer-file"');
    expect(TEAM_COMPOSER).toContain('📎');

    // Photo picker retired
    expect(TEAM_COMPOSER).not.toContain('id="composer-photo"');
    expect(TEAM_COMPOSER).not.toContain('📷');

    const roomsHtml = renderRoomsPage('team', { room: 'team', channels: ['team'], directRoles: [], messages: [] }, []);
    expect(roomsHtml).toContain('id="composer-file"');
    expect(roomsHtml).toContain('class="composer-file"');
    expect(roomsHtml).toContain('📎');
    expect(roomsHtml).not.toContain('id="composer-photo"');
    expect(roomsHtml).not.toContain('📷');

    // Negative proof: omitting composer-file from the room markup fails the assertion
    const brokenRoomHtml = roomsHtml.replace(/<label for="composer-file"[\s\S]*?<\/label>/, '');
    expect(brokenRoomHtml).not.toContain('id="composer-file"');
    expect(brokenRoomHtml).not.toContain('class="composer-file"');
  });

  test('fileNames extracts document uploads from message text while ignoring photos', () => {
    const text = 'Files: 📎 /clearing-v2-uploads/1700000000_arch.pdf, 📎 /clearing-v2-uploads/1793702849072_IMG_2082.jpeg, and log /clearing-v2-uploads/1700000001_run.txt';
    const names = fileNames(text);
    expect(names).toEqual(['1700000000_arch.pdf', '1700000001_run.txt']);
  });

  test('photoNames extracts photos attached via paperclip with timestamp prefixes and various formats', () => {
    const text = 'Here is the photo 📎 /Users/jeffbridwell/.chorus/clearing-v2-uploads/1793702849072_IMG_2082.jpeg and 📎 /clearing-v2-uploads/1793702849073_test.png';
    const photos = photoNames(text);
    expect(photos).toEqual(['1793702849072_IMG_2082.jpeg', '1793702849073_test.png']);
  });

  test('renderRoomMessage renders inline img for attached photos and download chip for files', () => {
    const photoMsg = {
      id: 'm1',
      room: 'team',
      from: 'jeff',
      text: 'Photo attached 📎 /clearing-v2-uploads/1793702849072_IMG_2082.jpeg',
      ts: '2026-11-02T19:00:00Z'
    };
    const photoHtml = renderRoomMessage(photoMsg as any);
    expect(photoHtml).toContain('msg-photos');
    expect(photoHtml).toContain('<img src="/uploads/1793702849072_IMG_2082.jpeg"');
    expect(photoHtml).not.toContain('msg-files');

    const fileMsg = {
      id: 'm2',
      room: 'team',
      from: 'jeff',
      text: 'Spec attached 📎 /clearing-v2-uploads/1700000000_spec.pdf',
      ts: '2026-11-02T19:01:00Z'
    };
    const fileHtml = renderRoomMessage(fileMsg as any);
    expect(fileHtml).toContain('msg-files');
    expect(fileHtml).toContain('1700000000_spec.pdf');
    expect(fileHtml).toContain('download');
    expect(fileHtml).not.toContain('msg-photos');
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

  test('NEGATIVE PROOF: DOM init wires watchFileButton and watchMicButton listeners, not watchPhotoButton', () => {
    const fs = require('fs');
    const path = require('path');
    const appJsContent = fs.readFileSync(path.join(__dirname, '../public/app.js'), 'utf8');

    // Extract the top-level script DOM init block
    const allMatches = Array.from(appJsContent.matchAll(/if\s*\(typeof document !== 'undefined'\)\s*\{([\s\S]*?)\}/g));
    const initBlock = allMatches.map((m: any) => m[1]).find((body: string) => body.includes('initComposerInput()'));
    expect(initBlock).toBeTruthy();

    expect(initBlock).not.toMatch(/\bwatchPhotoButton\(\)/);
    expect(initBlock).toMatch(/\bwatchFileButton\(\)/);
    expect(initBlock).toMatch(/\bwatchMicButton\(\)/);

    // Negative proof: an init body lacking watchFileButton fails this assertion
    const missingFile = initBlock!.replace(/watchFileButton\(\);?/, '');
    expect(missingFile).not.toMatch(/\bwatchFileButton\(\)/);

    const missingMic = initBlock!.replace(/watchMicButton\(\);?/, '');
    expect(missingMic).not.toMatch(/\bwatchMicButton\(\)/);
  });
});
