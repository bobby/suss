#[path = "/home/bobby/.codex-workspaces/worktrees/ios-bc59f46b-2529-47d6-9d06-a888fafcdd03/suss/target/probe/m5-usermanager/crates/suss-cli/tests/support/usermanager_canonical_stats.rs"]
mod canonical_stats;
use suss_usermanager_host::{Application, Department, Request, User};
fn main() {
 let original=std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
 let bytes=canonical_stats::instrument(&original);
 assert_eq!(&bytes[..original.len()], original.as_slice());
 let mut app=Application::load(&bytes).unwrap();
 let stats=app.instance.get_typed_func::<(),(u64,)>(&mut app.store,"canonical-stats").unwrap();
 let baseline=stats.call(&mut app.store,()).unwrap().0;
 assert_eq!(baseline,0);
 let db=app.open(":memory:").unwrap().unwrap();
 assert_eq!(stats.call(&mut app.store,()).unwrap().0,baseline);
 for size in [1000,10000] {
  // Host seeding isolates the large bidirectional canonical list path.
  // The separate normal measurement driver performs setup through guest CRUD.
  let state=app.store.data_mut().table.get_mut(&db).unwrap();
  state.database.users.clear();
  state.database.departments.insert(4,Department{id:4,name:"Development λ🦀".into()});
  for index in 0..size {
   let id=9007199254740993+index as i64;
   state.database.users.insert(id,User{id,first_name:Some(format!("Name {index:08} λ🦀")),last_name:Some(format!("Family {:08}",size-index)),email:Some(format!("user{index}@example.test")),department_id:Some(4)});
  }
  for repetition in 0..5 {
   let view=app.request(&db,&Request::ListUsers).unwrap().unwrap();
   assert_eq!(view.users.len(),size);
   assert_eq!(view.users[0].value.id,9007199254740993+size as i64-1);
   assert_eq!(view.users[0].department_name,"Development λ🦀");
   assert_eq!(stats.call(&mut app.store,()).unwrap().0,baseline);
   app.store.gc(None).unwrap();
   let observed=stats.call(&mut app.store,()).unwrap().0;
   assert_eq!(observed,baseline);
   println!("dataset={size} repetition={repetition} canonical-live-blocks-and-bytes={observed}");
  }
  app.store.data_mut().fail_next=Some("large-list failure".into());
  assert!(app.request(&db,&Request::ListUsers).unwrap().is_err());
  assert_eq!(stats.call(&mut app.store,()).unwrap().0,baseline);
 }
 app.close(db).unwrap();
 app.store.gc(None).unwrap();
 assert_eq!(stats.call(&mut app.store,()).unwrap().0,baseline);
 assert_eq!((app.store.data().opened,app.store.data().closed,app.store.data().live),(1,1,0));
 println!("PASS normal CLI component + exact-prefix observer; opened=1 closed=1 live=0; raw memory events={:?}", app.store.data().memory_events);
}
