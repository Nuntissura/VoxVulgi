import test from "node:test";
import assert from "node:assert/strict";
import { addTransferTransportAge, beginTransferExpiry, expireTransferRate, phase2TransferView, type Phase2Transfer } from "../src/lib/phase2Transfer.ts";
import { beginPhase2TransferPolling, type Phase2TransferStatus } from "../src/lib/usePhase2Transfer.ts";
const owner = {job_id:"original",attempt_no:1,job_type:"install_phase2_packs_v1",job_status:"running",step_id:"model",step_status:"running"};
const value: Phase2Transfer = {source:"pip_raw",job_id:"original",attempt_no:1,step_id:"model",command_id:"command",child_pid:1,measurement_age_ms:0,received_bytes:600,baseline_bytes:500,newly_received_bytes:100,total_bytes:1000,bytes_per_second:50,remaining_transfer_seconds:8};
test("shared transfer distinguishes resumed baseline and current transfer ETA", () => {
  const view=phase2TransferView(value,owner); assert.equal(view.fraction,0.6);assert.match(view.text,/50 B\/s/);assert.match(view.text,/transfer ETA 8 s/);
});

test("transfer display scales bytes and speed without rounding the measured fraction", () => {
  const observed={...value,received_bytes:1500000,baseline_bytes:1000000,newly_received_bytes:500000,total_bytes:2000000000,bytes_per_second:2500000,remaining_transfer_seconds:90};
  const view=phase2TransferView(observed,owner);
  assert.equal(view.fraction,1500000/2000000000);
  assert.match(view.text,/1[.,]5 MB \/ 2 GB/);
  assert.match(view.text,/2[.,]5 MB\/s/);
  assert.match(view.text,/transfer ETA 1 min 30 s/);
  const kilobytes=phase2TransferView({...value,received_bytes:1000,baseline_bytes:500,newly_received_bytes:500,total_bytes:2000,bytes_per_second:1000,remaining_transfer_seconds:60},owner);
  assert.match(kilobytes.text,/1 KB \/ 2 KB; 1 KB\/s; transfer ETA 1 min/);
  assert.match(phase2TransferView({...value,bytes_per_second:0.25},owner).text,/0[.,]25 B\/s/);
});
test("unknown total and stale rate remain indeterminate without ETA", () => {
  const view=phase2TransferView({...value,total_bytes:null,bytes_per_second:null,remaining_transfer_seconds:8},owner);assert.equal(view.fraction,null);assert.doesNotMatch(view.text,/ETA|B\/s/);
});
test("canonical attempt, terminal, wrong job and impossible counters refuse measurements", () => {
  for(const next of [{...owner,attempt_no:2},{...owner,job_status:"succeeded"},{...owner,job_id:"other"}]) assert.equal(phase2TransferView(value,next).fraction,null);
  assert.equal(phase2TransferView({...value,newly_received_bytes:600},owner).fraction,null);
});
test("actual shared poller rejects pending read after visibility cleanup or owner switch", async () => {
  let resolve!: (value: Phase2TransferStatus) => void;
  const pending=new Promise<Phase2TransferStatus>(r=>{resolve=r;});const commits: unknown[]=[];
  const stop=beginPhase2TransferPolling("original",()=>pending,v=>commits.push(v),()=>()=>{});
  stop();resolve({canonical_install:{id:"original",job_type:owner.job_type,attempt_no:1,status:"running",progress:0,error:null},owner,transfer:value});
  await pending;await Promise.resolve();assert.deepEqual(commits,[]);
});
test("actual shared poller clears mismatched canonical read and stops on genuine terminal", async () => {
  const commits: unknown[]=[];let schedules=0;
  const terminal: Phase2TransferStatus={canonical_install:{id:"original",job_type:owner.job_type,attempt_no:1,status:"succeeded",progress:1,error:null},owner:null,transfer:null};
  const stop=beginPhase2TransferPolling("original",async()=>terminal,v=>commits.push(v),()=>{schedules++;return()=>{};});
  await Promise.resolve();await Promise.resolve();assert.deepEqual(commits,[terminal]);assert.equal(schedules,0);stop();
  const stopWrong=beginPhase2TransferPolling("new",async()=>terminal,v=>commits.push(v),()=>{schedules++;return()=>{};});
  await Promise.resolve();await Promise.resolve();assert.equal(commits.at(-1),null);assert.equal(schedules,1);stopWrong();
});

test("model manual exposes exact read-only transfer projection without mutations", async () => {
  const {readFile}=await import("node:fs/promises");const manual=JSON.parse(await readFile(new URL("../src/lib/agentManual.json",import.meta.url),"utf8"));
  const command=manual.commands.find((c:any)=>c.name==="tools.phase2_transfer_status");assert.equal(command.read_only,true);assert.equal(command.effect,"read_only");
  assert.equal(command.input_schema.properties.command.const,command.name);assert.equal(command.input_schema.additionalProperties,false);
  assert.deepEqual(Object.keys(command.input_schema.properties).sort(),["actor_id","bridge_token","command","job_id"]);
});
test("independent source-age expiry retains bytes while a subsequent read is pending", () => {
  let run!:()=>void;let delay=-1;let expired=false;let canceled=false;
  const original={...value,measurement_age_ms:2500};
  const stop=beginTransferExpiry(original,()=>{expired=true;},(callback,ms)=>{run=callback;delay=ms;return()=>{canceled=true;};});
  assert.equal(delay,501);run();assert.equal(expired,true);
  const stale=expireTransferRate(original,501)!;
  assert.equal(stale.received_bytes,600);assert.equal(stale.bytes_per_second,null);assert.equal(stale.remaining_transfer_seconds,null);
  assert.equal(expireTransferRate({...value,measurement_age_ms:null},0)!.bytes_per_second,null);
  stop();assert.equal(canceled,true);
});

test("actual poller conservatively counts IPC delay before committing source measurement", async () => {
  let now=0;let resolve!: (value:Phase2TransferStatus)=>void;
  const pending=new Promise<Phase2TransferStatus>(r=>{resolve=r;});
  const commits: (Phase2TransferStatus|null)[]=[];
  const stop=beginPhase2TransferPolling("original",()=>pending,v=>commits.push(v),()=>()=>{},()=>now);
  now=4000;
  resolve({canonical_install:{id:"original",job_type:owner.job_type,attempt_no:1,status:"running",progress:0,error:null},owner,transfer:value});
  await pending;await Promise.resolve();
  const observed=commits[0]!.transfer!;
  assert.equal(observed.received_bytes,600);assert.equal(observed.measurement_age_ms,4000);
  assert.equal(observed.bytes_per_second,null);assert.equal(observed.remaining_transfer_seconds,null);
  stop();
  for(const bad of [NaN,Infinity,-1]) {
    const refused=addTransferTransportAge(value,bad)!;
    assert.equal(refused.received_bytes,600);assert.equal(refused.measurement_age_ms,null);assert.equal(refused.bytes_per_second,null);
  }
});
