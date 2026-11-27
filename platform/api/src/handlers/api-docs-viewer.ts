/**
 * #5042 — a generated API's OpenAPI document, drawn as a page you can read.
 *
 * Jeff, 2026-11-27 16:26, after the API Contract link opened the document itself:
 * "This is not open api to me… giving me a link to json is crap". The page is drawn
 * by swagger-ui-express, the viewer Gathering already serves at /api-docs, from the
 * document the generated API serves (athena-gen's surfaces/<Class>.openapi.json). No
 * new generator: this only draws what is served.
 *
 * Read-only: "Try it out" is off (no submit methods), so the page cannot send a request.
 * It lives under /athena, which the public door's allow-list already carries, and the
 * document is read server side through the same /owl routing, so the page names no
 * address of its own.
 */
import type { NextFunction, Request, Response } from 'express';
import swaggerUi from 'swagger-ui-express';

export const API_DOCS_MOUNT = '/athena/api-docs';

/** One name segment of a route: what a Domain or a collection is called. */
const SEGMENT = /^[a-z0-9][a-z0-9-]{0,63}$/;

/** The document's path through the /owl proxy, or null when either name is not a name. */
export function documentPath(domain: string, collection: string): string | null {
  return SEGMENT.test(domain) && SEGMENT.test(collection) ? `/v1/${domain}/${collection}/openapi.json` : null;
}

/** The viewer's options: read-only, nothing offered that would send a request. */
export const VIEWER_OPTIONS = {
  customSiteTitle: 'API',
  customCss: '.swagger-ui .topbar { display: none }',
  swaggerOptions: { supportedSubmitMethods: [] as string[], tryItOutEnabled: false },
};

export type ReadDocument = (owlPath: string) => Promise<{ status: number; body: unknown }>;

function page(status: number, text: string): [number, string] {
  const esc = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  return [status, `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>API</title><p class="api-docs-refused">${esc}</p>`];
}

/**
 * Express handlers for `${API_DOCS_MOUNT}/:domain/:collection`: the document is read for the
 * page and for its init script; the viewer's own files are served as they are.
 */
export function apiDocsViewer(read: ReadDocument) {
  const loadDocument = async (req: Request, res: Response, next: NextFunction) => {
    const owlPath = documentPath(String(req.params.domain), String(req.params.collection));
    if (!owlPath) {
      const [s, b] = page(404, 'no such API');
      return res.status(s).type('html').send(b);
    }
    // the viewer's assets are relative to the page, so the page's address ends in a slash
    if (req.path === '/' && !req.originalUrl.split('?')[0].endsWith('/')) {
      return res.redirect(301, `${String(req.params.collection)}/`);
    }
    const wantsDocument = req.path === '/' || req.path.endsWith('/swagger-ui-init.js');
    if (!wantsDocument) return next();
    try {
      const { status, body } = await read(owlPath);
      const doc = body as { openapi?: unknown } | null;
      if (status !== 200 || !doc || typeof doc.openapi !== 'string') {
        const [s, b] = page(status === 404 ? 404 : 502, `no OpenAPI document at ${owlPath} (${status})`);
        return res.status(s).type('html').send(b);
      }
      (req as Request & { swaggerDoc?: unknown }).swaggerDoc = doc;
      return next();
    } catch {
      const [s, b] = page(502, `${owlPath} could not be read`);
      return res.status(s).type('html').send(b);
    }
  };
  return [loadDocument, ...swaggerUi.serveFiles(undefined, VIEWER_OPTIONS), swaggerUi.setup(undefined, VIEWER_OPTIONS)];
}
