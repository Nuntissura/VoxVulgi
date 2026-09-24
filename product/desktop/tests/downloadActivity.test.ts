import test from "node:test";
import assert from "node:assert/strict";
import { activityFailure, activityTitle, activityStage, transferPercent, type ActivityRow } from "../src/lib/downloadActivity.ts";
const row: ActivityRow = {job:{id:"one",target_title:null,job_type:"download_direct_url",status:"running",progress:0.7,error:null,params_json:"{}",track:"youtube_single"},live:null};
// WP-0322: app-acts classes (app_busy, youtube_blocked, youtube_not_responding) now DO promise an
// automatic retry, because the engine (Scope B1) guarantees a bounded automatic retry for exactly
// these classes. An unclassified error must still never invent an unproven promise.
test("only engine-guaranteed app-acts classes promise an automatic retry",()=>{
  for (const error of ["network timeout", "HTTP Error 429", "database is locked"])
    assert.match(activityFailure(error),/automatically/);
  assert.doesNotMatch(activityFailure("unknown fault"),/automatically|no action needed/);
});
test("unknown transfer does not turn the engine's reserved progress into byte progress",()=>{
  assert.equal(transferPercent(null),null);
  assert.match(activityStage(row),/unavailable/);
  assert.equal(activityStage({...row,job:{...row.job,status:"queued"}}),"Waiting to start");
});
test("provider bytes define progress and postprocessing cannot look completed",()=>{
  const live={updated_at_ms:1,phase:"downloading",title:"Actual video",downloaded_bytes:25,total_bytes:100,speed:10,eta:8,lines:[]};
  assert.equal(transferPercent(live),25);
  assert.equal(transferPercent({...live,total_bytes:null}),null);
  assert.equal(transferPercent({...live,phase:"processing"}),null);
  assert.equal(activityTitle({...row,live}),"Actual video");
  assert.equal(activityStage({...row,live:{...live,phase:"processing"}}),"Preparing the saved media");
});
test("metadata fallbacks stay useful without exposing authenticated URL details",()=>{
  const job={...row.job,params_json:JSON.stringify({url:"https://user:secret@example.com/media?id=123&token=private"})};
  assert.equal(activityTitle({...row,job}),"example.com/media");
  for(const status of ["failed","queued","canceled","succeeded"]){
    assert.equal(activityTitle({...row,job:{...job,status,target_title:"Saved title"}}),"Saved title");
  }
});
