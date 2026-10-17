import assert from "node:assert/strict";
import test from "node:test";
import {clampFriendsSidebar, canJoinFriend, friendCounts, reconcileFriendsSelection, visibleFriends} from "../frontend/lib/friends.ts";

test("friend details resizing keeps a useful list width and clamps both limits",()=>{
  assert.equal(clampFriendsSidebar(100,1200),240);
  assert.equal(clampFriendsSidebar(900,1200),520);
  assert.equal(clampFriendsSidebar(500,800),372);
  assert.equal(clampFriendsSidebar(320,1000),320);
});
const friend=(userId,name,status,jobId=null)=>({userId,displayName:name,username:`user${userId}`,status,jobId,avatarUrl:"",activity:"",placeId:null,gameName:null,gameIcon:null});
const friends=[friend(1,"Zed","playing","12345678-1234-4234-8234-123456789abc"),friend(2,"Beta","studio"),friend(3,"Alpha","online"),friend(4,"Delta","offline"),friend(5,"Echo","unknown")];
test("friend counts classify every friend exactly once",()=>{
 const counts=friendCounts(friends);assert.deepEqual(counts,{all:5,playing:1,studio:1,online:1,offline:1,unknown:1});assert.equal(counts.playing+counts.studio+counts.online+counts.offline+counts.unknown,counts.all);
});
test("friend search is local and matches display names and usernames",()=>{
 assert.deepEqual(visibleFriends(friends,"  ALPHA ","all","status").map(friend=>friend.userId),[3]);
 assert.deepEqual(visibleFriends(friends,"user2","all","status").map(friend=>friend.userId),[2]);
 assert.equal(visibleFriends(friends,"nomatch","all","status").length,0);
});
test("status filters and every supported sort preserve the input",()=>{
 assert.deepEqual(visibleFriends(friends,"","playing","status").map(friend=>friend.userId),[1]);
 assert.deepEqual(visibleFriends(friends,"","all","status").map(friend=>friend.userId),[1,2,3,4,5]);
 assert.deepEqual(visibleFriends(friends,"","all","nameAsc").map(friend=>friend.userId),[3,2,4,5,1]);
 assert.deepEqual(visibleFriends(friends,"","all","nameDesc").map(friend=>friend.userId),[1,5,4,2,3]);
 assert.deepEqual(visibleFriends(friends,"","all","userAsc").map(friend=>friend.userId),[1,2,3,4,5]);
 assert.deepEqual(visibleFriends(friends,"","all","userDesc").map(friend=>friend.userId),[5,4,3,2,1]);
 assert.deepEqual(friends.map(friend=>friend.userId),[1,2,3,4,5]);
});
test("selection survives filtering and drops removed friends after refresh",()=>{
 const selected=new Set([1,2,99]);assert.deepEqual([...reconcileFriendsSelection(selected,friends)],[1,2]);assert.deepEqual([...selected],[1,2,99]);
 assert.deepEqual([...reconcileFriendsSelection(selected,[])],[]);
});
test("only playing friends offer a supported join lookup, including hidden Job IDs",()=>{
 assert(canJoinFriend(friends[0]));assert(canJoinFriend(friend(6,"Hidden server","playing")));
 for(const friend of friends.slice(1))assert(!canJoinFriend(friend));
 assert.equal(friends[0].jobId,"12345678-1234-4234-8234-123456789abc");
});
