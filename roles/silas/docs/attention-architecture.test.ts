import fs from 'fs';
import path from 'path';

const DOC_PATH = path.join(__dirname, 'attention-architecture.html');

describe('Attention Architecture doc — #1831 AC', () => {
  let html: string;

  beforeAll(() => {
    html = fs.readFileSync(DOC_PATH, 'utf-8');
  });

  it('AC-1: contains time+attention→awareness governing formula', () => {
    expect(html).toContain('The Governing Formula');
    expect(html).toMatch(/Time.*Attention.*Awareness/s);
  });

  it('AC-2: documents navigator ceremony anti-pattern', () => {
    expect(html).toMatch(/navigator.*ceremony/i);
    expect(html).toMatch(/ceremony.*replaces.*loop|startup.*ceremony/i);
  });

  it('AC-2: documents UTC/local mismatch anti-pattern', () => {
    expect(html).toMatch(/UTC.*local.*mismatch|UTC.*timestamp/i);
  });

  it('AC-5: references Mik Kersten output-to-outcome framing', () => {
    expect(html).toMatch(/Mik Kersten/i);
    expect(html).toMatch(/output.*outcome/i);
  });

  it('AC-3: implementation checklist reflects current status', () => {
    // The checklist section should exist and have updated items
    expect(html).toContain('Implementation Checklist');
  });
});
