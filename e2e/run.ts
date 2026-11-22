import { mkdir, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { backgroundRequested, ChannelDriver } from "./channel.ts";
import { JOURNEYS } from "./journeys.ts";
import { claimRunFolder, removeAbandonedRuns, sweepReport } from "./scratch.ts";

const shell = process.env.WB_E2E_SHELL;
if (shell === undefined) {
  console.error("set WB_E2E_SHELL to a shell built with the automation feature");
  process.exit(2);
}

for (const line of sweepReport(await removeAbandonedRuns())) {
  console.log(`      ${line}`);
}

const scratch = await claimRunFolder();
const keptArtifacts = process.env.WB_E2E_ARTIFACTS;
const artifacts = keptArtifacts === undefined ? join(scratch, "artifacts") : resolve(keptArtifacts);
await mkdir(artifacts, { recursive: true });

const drivers = new Set<ChannelDriver>();
const disposeAll = () => Promise.all([...drivers].map((driver) => driver.dispose()));
const removeScratch = () => rm(scratch, { recursive: true, force: true });
for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => {
    void disposeAll()
      .finally(removeScratch)
      .finally(() => process.exit(130));
  });
}

let failures = 0;
for (const journey of JOURNEYS) {
  const driver = new ChannelDriver(shell, backgroundRequested(), scratch);
  drivers.add(driver);
  const started = performance.now();
  let failure: unknown = null;
  try {
    await journey.run(driver, artifacts);
  } catch (error) {
    failure = error;
  } finally {
    await driver.dispose();
    drivers.delete(driver);
  }
  const seconds = ((performance.now() - started) / 1000).toFixed(2);
  if (failure === null) {
    console.log(`      journey  ${journey.name}: pass in ${seconds}s`);
  } else {
    failures += 1;
    const reason = failure instanceof Error ? failure.message : String(failure);
    console.log(`      journey  ${journey.name}: FAIL in ${seconds}s\n      ${reason}`);
  }
}

await removeScratch();
process.exitCode = failures === 0 ? 0 : 1;
