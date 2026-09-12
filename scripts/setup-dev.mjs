import pg from "pg";
import { randomBytes, createHash } from "node:crypto";
import { readFile, writeFile, mkdir, readdir } from "node:fs/promises";
const directory = process.env.AIO_AGENT_DEV_DIRECTORY || ".local";
await mkdir(directory, { recursive: true, mode: 0o700 });
let existing;
try {
  existing = JSON.parse(await readFile(`${directory}/runtime.json`, "utf8"));
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
  schema = existing
    ? new URL(existing.databaseUrl).username.replace(/^r_/, "")
    : `agent_dev_${suffix}`,
  role = `r_${schema}`,
  owner = `o_${schema}`,
  password = randomBytes(32).toString("hex");
try {
  if (!/^agent_dev_[a-f0-9]{20}$/.test(schema))
    throw new Error("Invalid development schema");
  if (existing) {
    const prior = new URL(existing.databaseUrl),
      target = new URL(process.env.AIO_TEST_DATABASE_URL);
    if (prior.host !== target.host || prior.pathname !== target.pathname)
      throw new Error("Existing configuration belongs to another database");
  }
  await admin.query("BEGIN");
  await admin.query("SELECT pg_advisory_xact_lock(hashtext($1))", [schema]);
  if (!existing) {
    await admin.query(
      `CREATE ROLE ${owner} NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS`,
    );
    await admin.query(
      `CREATE ROLE ${role} LOGIN PASSWORD '${password}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT CONNECTION LIMIT 6`,
    );
    await admin.query(`CREATE SCHEMA ${schema} AUTHORIZATION ${owner}`);
  }
  await admin.query(`SET LOCAL ROLE ${owner}`);
  await admin.query(`SET LOCAL search_path TO ${schema}`);
  await admin.query(
    "CREATE TABLE IF NOT EXISTS agent_schema_migrations (name TEXT PRIMARY KEY, checksum TEXT NOT NULL)",
  );
  const files = (await readdir("backend/migrations"))
    .filter((name) => /^\d+_[a-z_]+\.sql$/.test(name))
    .sort();
  for (const name of files) {
    const sql = await readFile(`backend/migrations/${name}`, "utf8");
    const checksum = createHash("sha256").update(sql).digest("hex");
    const applied = await admin.query(
      "SELECT checksum FROM agent_schema_migrations WHERE name=$1",
      [name],
    );
    if (applied.rows.length) {
      if (applied.rows[0].checksum !== checksum)
        throw new Error(`Applied migration changed: ${name}`);
      continue;
    }
    const legacy = existing && name === "0001_agent.sql";
    if (!legacy) await admin.query(sql);
    await admin.query("INSERT INTO agent_schema_migrations VALUES ($1,$2)", [
      name,
      checksum,
    ]);
  }
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
  if (existing) {
    console.log(
      "Development migrations applied; existing keys and database binding retained",
    );
    process.exitCode = 0;
  } else {
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
  }
} catch (error) {
  await admin.query("ROLLBACK");
  throw error;
} finally {
  await admin.end();
}
