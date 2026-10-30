# Infestation process-task relation queries

This authority owns the virtual `+0` relation queries on the nineteen process
task allocations at `0055F100[19]`. Their storage, registration and lifetime
remain with [process task storage](ACTOR_RUNTIME.md#process-task-storage).
These queries are shared actor services; neither the mission nor the placement
selects a replacement policy.

[relation.rs](../../crates/inf-game/src/process_tasks/relation.rs) implements
the queries through `TaskRelationRequest`, retaining a stable `TaskOwner` and
the queried actor identity. `TaskRelationContext` passes the actual mutable
task and arena owners to required component services. `TaskRelationHost` reads
the current network flag through a pure scalar read only at its reached load;
it does not retain a copy. The component `+7C` result is tested as a full DWORD,
so a nonzero high byte is not mistaken for null.

| Registered classification | Final callback | Policy |
| --- | --- | --- |
| 0 | `004027B0` | constant zero |
| 1 | `004EFBF0` | constant zero |
| 18 | `00482C90` | component classification |
| 2 through 17 | `00482CE0` | team classification |

The virtual call captures the actual task allocation and callback identity.
A task's registration slot is separate from that allocation. A later slot
replacement does not retarget the current query. The queries do not simulate
tasks, reset their unwritten counters or update their source handle.

## Constant and team callbacks

Both constant callbacks return zero without reading the actor, network flag,
or any task field inside their body. Portable dispatch still requires a live
task allocation to identify the actual callback.

`00482CE0` first reads the actor's DWORD classification `+B4`. Classification
18 returns two before reading network flag `0055F448`. Otherwise the flag is
read fresh. A zero flag returns zero. With a nonzero flag, actor classifications
zero and one return zero before reading task `+04`. The remaining actors
return zero when their classification equals the task's current `+04`, and
two otherwise. Classification comparison is DWORD equality, not an array
index or signed range policy.

The task source `+08`, lifecycle `+C4`, actor position and behavior are not
inputs to the team callback. A missing network value is a reached boundary
only on the branch that actually reads it.

## Component callback

`00482C90` first captures actor classification `+B4`. Zero and 18 return zero;
any classification other than one returns two. Classification one invokes
the actor's actual virtual `+5C`:

| `+5C` result | Captured component's `+7C` result | Returned relation |
| --- | --- | --- |
| null | not called | 1 |
| nonnull | null | 0 |
| nonnull | nonnull | 1 |

The nonnull `+5C` component identity is retained for the second query. Actor
classification is not reread after either callback. Neither the task owner,
its registration slot nor its fields are dereferenced after these calls.
Services may therefore mutate the actual task/arena state or remove the
captured task allocation without inventing a later validation read.

The component methods remain required synchronous services, with the actual
mutable task and arena owners passed to them. Owning this query kernel does
not automatically own every actor's `+5C` or every component's `+7C`.
Missing services stop at their reached call and retain earlier mutations.

## Callers and remaining boundaries

The common family `+104` callback `00428F40` queries the actor's captured task
against each candidate, then the candidate's freshly selected task against
the actor. The separately owned
[common actor-list selection policy](FAMILY_TARGET_SELECTION.md#common-actor-list-selection-callback)
composes these relations with its three-axis WORD-wrapped distance, group
averaging, weighted scoring and three publishers. Its pure canonical query
bridge borrows the actual task/arena/network owners briefly before the mutable
candidate gate. Broader required component services remain this task host's
standalone boundary. The [common `+100` scanner](FAMILY_TARGET_SELECTION.md)
retains its distinct distance and selection policy.
The separately owned
[selector-12 `+100` policy](FAMILY_TARGET_SELECTION.md#selector-12-inline-table-selection-callback)
uses no task relation, task-source read or classification-based dispatch;
its per-row pure `+7C` and direct `+140` checks retain their own order.

The [selector-12 actor-list policy](FAMILY_TARGET_SELECTION.md#selector-12-actor-list-selection-callback)
`+104 = 00468350` queries each candidate's freshly selected class task against
self, without an outer captured task, reciprocal relation or task-source `+08`
read. The same pure canonical bridge ends its borrow before the gate; a later
row can use a genuinely replaced task owner. Selector 13's identical static
callback retains its unrepresented constructor boundary.
Production dispatch, broader component query admission and complete actor
update/cinematic acceptance remain open. Isolated relation-query evidence
does not establish complete intro or briefing behavior.

## Evidence and admission

check_process_task_relation.py executes
the unchanged four relation bodies and authentic final-vtable query paths:
family `00520F34 +5C = 00401300` returns null, while selector-12/13 tables
`00527058/005269C4 +5C = 00404610` return self. Their `+7C = 004027A0` reads
the current DWORD `+140`. Supplied raw fields admit those exact query bodies;
this does not add missing actor constructors or production query dispatch.

The [structured fixture](../../crates/inf-game/tests/fixtures/process_task_relation_native.json)
has 42 histories/42 steps: 27 authentic kernel histories, two kernels with
supplied lower queries, five semantic prefixes and eight source-only
diagnostics. It records 60 reached original routine entries, 17 original query
returns and five supplied lower-query entries. These counts distinguish a
reached body from a completed query and never count supplied service returns
as execution of their original body. EXE, unchanged text, final-vtable words
and FPCW `027F` are authenticated.

Four mutable-service histories explicitly supply or stop reached `+5C/+7C`
boundaries. Their earlier mutations and captured identities are query-kernel
evidence; they do not prove those lower bodies produce the supplied effects.
Task destruction in these services supplies owner retirement and registration
custody. It does not execute or prove destructor `00482980` or a native free.
Retired physical task bytes are stale backing, not a live portable allocation.

Eight named diagnostics retain unknown actor/task/vtable/behavior fields and
unmapped pointers outside portable admission. This includes unknown fields
bypassed by a null query or disabled network, whose absence of consumption
does not make them known null or zero. The five admitted prefixes compare a
missing network value or unavailable query service with retained prior state;
they do not claim raw fault or stop-IP parity.

The eight [focused Rust tests](../../crates/inf-game/src/process_tasks/relation/tests.rs)
pin live dispatch-owner admission, skipped reads, full DWORD flags/results,
captured classification/component custody, callback task retirement and both
failure prefixes. The [native comparison](../../crates/inf-game/src/process_tasks/relation/native_receipts.rs)
retains the 34 admitted histories separately from the eight explicitly named
diagnostics. Supplied lower-query backing preserves exact vtable/`+140` DWORDs
and masks; the concrete receipt host evaluates the authenticated query bodies
from these inputs. Complete portable actor states, current task registration,
source/counters and owner liveness are also compared. This scoped query host
does not claim a complete actor class from a supplied vtable identity.

```text
python check_process_task_relation.py --output .tmp/task-relation.json
cargo test --release -p inf-game --lib relation
```

Run Cargo from ``; the checker uses the existing
Python/Unicorn environment. No new retail capture or debugger replay is
required by this evidence.
