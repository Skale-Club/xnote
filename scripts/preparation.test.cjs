'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'),path=require('node:path'),vm=require('node:vm');
const src=process.env.XNOTE_SOURCE_DIR || path.join(__dirname,'..','src');
const flush=()=>new Promise(setImmediate);
async function harness(fullApp=false){
  const nodes=new Map(),timers=new Map(),calls=[],events={};let next=0;
  const node=(id)=>{if(!nodes.has(id))nodes.set(id,{innerHTML:'',textContent:'',hidden:true,disabled:false,value:'',dataset:{},style:{},attributes:{},classList:{add(){},remove(){},toggle(){}},setAttribute(k,v){this.attributes[k]=v;},addEventListener(){},focus(){}});return nodes.get(id);};
  let resolveStart,rejectStart;
  const start=new Promise((resolve,reject)=>{resolveStart=resolve;rejectStart=reject;});
  const engine={status:{gpu:true,packs:['cuda'],models:['best']},mode:'local',pack:'cuda',model:'best',cloudKeys:[]};
  const context=vm.createContext({console,Date,setInterval:()=>++next,clearInterval(){},setTimeout:(fn)=>{const id=++next;timers.set(id,fn);return id;},clearTimeout:(id)=>timers.delete(id),document:{documentElement:{},getElementById:node,querySelector:()=>node('ver'),querySelectorAll:()=>[],addEventListener(){},removeEventListener(){}},window:{crypto:{randomUUID:()=>`request-${++next}`},__TAURI__:{core:{invoke:async(command,args)=>{calls.push({command,args});if(command==='start_recording')return start;if(command==='cancel_recording_start')return true;return ({get_settings:{language:'en'},engine_status:engine,has_api_key:false,list_meetings:[],update_status:{current:'0.1.1',state:'idle',version:null,notes:'',error:null,checkedAt:null}})[command];}},event:{listen:async(name,fn)=>{events[name]=fn;return()=>{};}},app:{getVersion:async()=> '0.1.1',getName:async()=>'Xnote'}}}});
  vm.runInContext(fs.readFileSync(path.join(src,'i18n.js'),'utf8'),context);
  vm.runInContext(fs.readFileSync(path.join(src,'preparation.js'),'utf8'),context);
  if(fullApp){vm.runInContext(fs.readFileSync(path.join(src,'updates.js'),'utf8'),context);vm.runInContext(fs.readFileSync(path.join(src,'app.js'),'utf8'),context);await flush();vm.runInContext("meeting={id:'meeting-a',title:'Original meeting',durationMs:0,transcript:[],notes:'keep my notes'};view='meeting'",context);}
  const api=context.window.XnotePreparation;
  const options={requestId:'one',id:'meeting-a',title:'Meeting <script>',mode:'local',onCancel:async()=>true,onRetry:()=>{}};
  const click=(action)=>node('preparation').onclick({target:{closest:()=>({dataset:{prep:action},disabled:false})}});
  return{api,context,nodes,node,calls,events,timers,options,click,resolveStart,rejectStart,html:()=>node('preparation').innerHTML};
}

test('progress counts actual completed stages and never displays a model percentage',async()=>{
  const h=await harness();h.api.begin(h.options);assert.match(h.html(),/aria-valuemax="4" aria-valuenow="0"/);assert.match(h.html(),/Recording has not started/);
  h.api.update({requestId:'one',stage:'loading'});assert.match(h.html(),/width:25%/);assert.match(h.html(),/Step 2 of 4/);
  h.api.update({requestId:'one',stage:'audio'});assert.match(h.html(),/width:75%/);h.api.done('one');assert.match(h.html(),/width:100%/);assert.doesNotMatch(h.html(),/Recording has not started/);
});
test('stale or backward events cannot alter a current request',async()=>{
  const h=await harness();h.api.begin(h.options);h.api.update({requestId:'one',stage:'connecting'});const html=h.html();
  h.api.update({requestId:'other',stage:'audio'});h.api.update({requestId:'one',stage:'loading'});h.api.fail('other','bad');assert.equal(h.html(),html);
});
test('cloud preparation skips the model and local-engine stages',async()=>{
  const h=await harness();h.api.begin({...h.options,mode:'cloud'});h.api.update({requestId:'one',stage:'audio'});assert.match(h.html(),/Step 2 of 2/);assert.match(h.html(),/width:50%/);assert.doesNotMatch(h.html(),/Load model/);
});
test('cancel stays visible until the worker confirms it actually ended',async()=>{
  const h=await harness();let token;h.api.begin({...h.options,onCancel:async(id)=>{token=id;}});h.click('cancel');await flush();assert.equal(token,'one');assert.match(h.html(),/Cancelling/);assert.equal(h.node('preparation').hidden,false);
  h.api.close();assert.equal(h.node('preparation').hidden,true);assert.equal(h.node('new-meeting').disabled,false);
});
test('failure preserves details, escapes untrusted output and offers retry',async()=>{
  const h=await harness();let retry=0;h.api.begin({...h.options,onRetry:()=>retry++});h.api.fail('one','engine_start_timeout|<script>fixture</script>');
  assert.match(h.html(),/three minutes/);assert.match(h.html(),/data-prep="retry"/);assert.doesNotMatch(h.html(),/<script>fixture/);assert.match(h.html(),/&lt;script&gt;/);h.click('retry');assert.equal(retry,1);
});
test('a stale completion timer cannot hide a later preparation',async()=>{
  const h=await harness();h.api.begin(h.options);h.api.done('one');const old=[...h.timers.values()][0];h.api.begin({...h.options,requestId:'two'});old();assert.equal(h.node('preparation').hidden,false);assert(h.api.matches('two'));
});
test('the command result starts the UI even without a recording-state event',async()=>{
  const h=await harness(true);h.node('notes').value='untouched';const pending=vm.runInContext('startRecording()',h.context);await flush();
  const call=h.calls.find(c=>c.command==='start_recording');assert.equal(call.args.id,'meeting-a');assert(call.args.requestId);assert.match(h.html(),/Recording has not started/);
  h.events['recording-preparation']({payload:{requestId:call.args.requestId,stage:'connecting'}});assert.equal(h.node('notes').value,'untouched');h.resolveStart([]);await pending;
  assert.equal(vm.runInContext('recording.phase',h.context),'recording');assert.match(h.html(),/Recording started/);
});
test('cancel from the real app uses the same request ID and leaves no recording',async()=>{
  const h=await harness(true);const pending=vm.runInContext('startRecording()',h.context);await flush();h.click('cancel');await flush();
  const start=h.calls.find(c=>c.command==='start_recording'),cancel=h.calls.find(c=>c.command==='cancel_recording_start');assert.equal(cancel.args.requestId,start.args.requestId);
  h.rejectStart('startup_cancelled');await pending;assert.equal(vm.runInContext('recording',h.context),null);assert.equal(h.node('preparation').hidden,true);
});
test('preparation translations have equal keys and placeholders in EN/PT/ES',async()=>{
  const h=await harness();const text=vm.runInContext('I18N',h.context),keys=Object.keys(text.en.ui).filter(k=>k.startsWith('prep')).sort();
  for(const lang of ['pt','es']){assert.deepEqual(Object.keys(text[lang].ui).filter(k=>k.startsWith('prep')).sort(),keys);for(const key of keys)assert.deepEqual([...text[lang].ui[key].matchAll(/\{(\w+)\}/g)].map(m=>m[1]).sort(),[...text.en.ui[key].matchAll(/\{(\w+)\}/g)].map(m=>m[1]).sort());}
});
