import { Registry, Counter, Histogram, collectDefaultMetrics } from 'prom-client';
import * as http from 'http';
import { log } from './logger';

const register = new Registry();
collectDefaultMetrics({ register });

export const metrics = {
  messagesReceived: new Counter({
    name: 'slack_bridge_messages_received_total',
    help: 'Total messages received from Slack',
    labelNames: ['channel', 'role'] as const,
    registers: [register],
  }),

  responsesSent: new Counter({
    name: 'slack_bridge_responses_sent_total',
    help: 'Total responses posted to Slack',
    labelNames: ['channel', 'role'] as const,
    registers: [register],
  }),

  apiCalls: new Counter({
    name: 'slack_bridge_api_calls_total',
    help: 'Total Claude API calls',
    labelNames: ['role'] as const,
    registers: [register],
  }),

  apiLatency: new Histogram({
    name: 'slack_bridge_api_latency_seconds',
    help: 'Claude API response latency',
    labelNames: ['role'] as const,
    buckets: [0.5, 1, 2, 5, 10, 30],
    registers: [register],
  }),

  rateLimited: new Counter({
    name: 'slack_bridge_rate_limited_total',
    help: 'Total rate-limited requests',
    labelNames: ['role'] as const,
    registers: [register],
  }),

  errors: new Counter({
    name: 'slack_bridge_errors_total',
    help: 'Total errors by type',
    labelNames: ['type'] as const,
    registers: [register],
  }),
};

export function startMetricsServer(port: number): http.Server {
  const server = http.createServer(async (req, res) => {
    if (req.url === '/metrics') {
      res.setHeader('Content-Type', register.contentType);
      res.end(await register.metrics());
    } else if (req.url === '/health') {
      res.setHeader('Content-Type', 'application/json');
      res.end(JSON.stringify({ status: 'ok', uptime: process.uptime() }));
    } else {
      res.statusCode = 404;
      res.end('Not Found');
    }
  });

  server.listen(port, () => {
    log('info', `Metrics/health server on port ${port}`);
  });

  return server;
}
