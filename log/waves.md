# Phase 1 wave partition (types with errors after triage)

Wave k: all type-graph dependencies are done or in an earlier wave. One worker per file group.


## Wave 1

- `src/code.rs`: 88 Expr (1)  deps: 
- `src/compiler.rs`: 52 Compiler (7)  deps: 
- `src/host.rs`: 33 HostName (1), 82 HostValList (4)  deps: 
- `src/interpreter.rs`: 21 Interpreter (417)  deps: 
- `src/ir_reader.rs`: 23 IrReader (2)  deps: 
- `src/module.rs`: 18 DataSection (3), 43 Data (3)  deps: 
- `src/reader.rs`: 94 Reader (3)  deps: 
- `src/stack.rs`: 22 Stack (18)  deps: 
- `src/util/box_.rs`: 26 Box (14)  deps: 
- `src/util/circular_buffer.rs`: 64 Iter (4), 65 IterMut (2)  deps: 
- `src/util/paging.rs`: 28 Page (13)  deps: 
- `src/util/rc.rs`: 68 Rc (14)  deps: 
- `src/util/vec.rs`: 66 Vec (19)  deps: 

## Wave 2

- `src/code.rs`: 105 Func (1)  deps: 26, 88
- `src/host.rs`: 98 HostValListIter (2), 99 HostFunction (1)  deps: 26, 33, 82
- `src/memory.rs`: 102 Memory (50)  deps: 68
- `src/text.rs`: 92 CodeBuilder (6)  deps: 26, 66
- `src/util/paging.rs`: 72 PageAllocatorInner (12)  deps: 28

## Wave 3

- `src/module.rs`: 110 Module (9)  deps: 105, 26
- `src/util/paging.rs`: 95 PageAllocator (12)  deps: 72

## Wave 4

- `src/store.rs`: 111 Store (33)  deps: 102, 110, 66, 68

## Wave 5

- `src/engine.rs`: 114 Engine (8)  deps: 102, 111, 22, 68
- `src/text.rs`: 113 TextBuilder (18)  deps: 105, 110, 111, 92

## Dependency edges among error types

- 72 PageAllocatorInner <- 28 Page
- 92 CodeBuilder <- 66 Vec, 26 Box
- 95 PageAllocator <- 72 PageAllocatorInner
- 98 HostValListIter <- 82 HostValList
- 99 HostFunction <- 26 Box, 33 HostName, 82 HostValList
- 102 Memory <- 68 Rc
- 105 Func <- 26 Box, 88 Expr
- 110 Module <- 26 Box, 105 Func
- 111 Store <- 66 Vec, 68 Rc, 102 Memory, 110 Module
- 113 TextBuilder <- 92 CodeBuilder, 111 Store, 110 Module, 105 Func
- 114 Engine <- 68 Rc, 102 Memory, 22 Stack, 111 Store
