import assert from "node:assert/strict";
import test from "node:test";
import {exportValidation} from "../frontend/lib/accountExport.ts";
const defaults=()=>({accountIds:[1],fields:["username"],format:"json",encryption:"none",password:""});
test("metadata may be unencrypted but cookies always require recommended encryption",()=>{
  assert.equal(exportValidation(defaults(),""),null);
  const options={...defaults(),fields:["cookie"]};
  assert.match(exportValidation(options,""),/recommended encryption/);
  for(const encryption of ["aes256Gcm","aes128Gcm"]){
    options.encryption=encryption;options.password="synthetic passphrase";
    assert.equal(exportValidation(options,options.password),null);
    assert.match(exportValidation(options,"different"),/do not match/);
  }
});
test("empty account/field choices and short or oversized passwords block export",()=>{
  assert.match(exportValidation({...defaults(),accountIds:[]},""),/account/);
  assert.match(exportValidation({...defaults(),fields:[]},""),/field/);
  for(const password of ["short","            ","a".repeat(1025)]){
    assert.match(exportValidation({...defaults(),encryption:"aes256Gcm",password},password),/password/);
  }
});
