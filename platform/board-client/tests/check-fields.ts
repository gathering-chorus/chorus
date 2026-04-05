import { BoardClient } from '../src/client';
import { loadEnv, GATHERING } from '../src/config';

async function main() {
  const env = loadEnv();
  const client = new BoardClient(env.url, env.token, GATHERING);
  const tasks = await client.fetchAllTasks();
  const t = tasks[0] as any;
  console.log('Total tasks:', tasks.length);
  console.log('Fields:', Object.keys(t).join(', '));
  console.log('bucket_id:', t.bucket_id);
}
main().catch(e => console.error(e.message));
