import pg from "pg";
import { randomBytes } from "node:crypto";
import { readFile, writeFile, mkdir } from "node:fs/promises";
const directory = process.env.AIO_AGENT_DEV_DIRECTORY || ".local";
await mkdir(directory, { recursive: true, mode: 0o700 });
try {
  await readFile(`${directory}/runtime.json`);
  console.log("Existing development configuration retained");
  process.exit(0);
} catch (error) {
  if (error.code !== "ENOENT") throw error;
}
if (!process.env.AIO_TEST_DATABASE_URL)
  throw new Error(
    "AIO_TEST_DATABASE_URL must target a disposable development PostgreSQL database",
  );
const admin = new pg.Client({
  connectionString: process.env.AIO_TEST_DATABASE_URL,
});
await admin.connect();
const suffix = randomBytes(10).toString("hex"),
  schema = `agent_dev_${suffix}`,
  role = `r_${schema}`,
  owner = `o_${schema}`,
  password = randomBytes(32).toString("hex");
try {
  await admin.query("BEGIN");
  await admin.query(
    `CREATE ROLE ${owner} NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS`,
  );
  await admin.query(
    `CREATE ROLE ${role} LOGIN PASSWORD '${password}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT CONNECTION LIMIT 6`,
  );
  await admin.query(`CREATE SCHEMA ${schema} AUTHORIZATION ${owner}`);
  await admin.query(`SET LOCAL ROLE ${owner}`);
  await admin.query(`SET LOCAL search_path TO ${schema}`);
  await admin.query(
    await readFile("backend/migrations/0001_agent.sql", "utf8"),
  );
  await admin.query(`GRANT USAGE ON SCHEMA ${schema} TO ${role}`);
  await admin.query(
    `GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA ${schema} TO ${role}`,
  );
  await admin.query(
    `GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA ${schema} TO ${role}`,
  );
  await admin.query("RESET ROLE");
  await admin.query(`ALTER ROLE ${role} SET search_path TO ${schema}`);
  await admin.query("COMMIT");
  const url = new URL(process.env.AIO_TEST_DATABASE_URL);
  url.username = role;
  url.password = password;
  const settings = {
    databaseUrl: url.href,
    masterKey: randomBytes(32).toString("base64"),
    ingressToken: randomBytes(32).toString("hex"),
    allowedEndpoints: ["https://api.openai.com/v1"],
    allowLoopback: false,
  };
  await writeFile(
    `${directory}/runtime.json`,
    JSON.stringify(settings, null, 2) + "\n",
    { mode: 0o600 },
  );
  console.log(
    `Development schema created: ${schema}; credentials stay in ignored ${directory}/runtime.json`,
  );
} catch (error) {
  await admin.query("ROLLBACK");
  throw error;
} finally {
  await admin.end();
}
