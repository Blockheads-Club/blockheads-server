//! NSOperationQueue / NSBlockOperation shims (synchronous).
//!

use std::cell::Cell;
use std::rc::Rc;

/// `NSBlockOperation`.
#[derive(Default)]
pub struct BlockOperation {
    blocks: Vec<Box<dyn FnOnce()>>,
}

impl BlockOperation {
    /// `-init`: empty block array.
    pub fn new() -> Self {
        BlockOperation { blocks: Vec::new() }
    }
    /// `-addExecutionBlock:`.
    pub fn add_execution_block(&mut self, f: Box<dyn FnOnce()>) {
        self.blocks.push(f);
    }
    /// `-executionBlocks` count.
    pub fn execution_block_count(&self) -> usize {
        self.blocks.len()
    }
    /// `-main`: run every block in insertion order, then clear.
    pub fn main(mut self) {
        for b in std::mem::take(&mut self.blocks) {
            b();
        }
    }
}

/// `NSOperationQueue` that executes operations on the caller's thread at enqueue time.
#[derive(Clone, Default)]
pub struct OpQueue {
    executed: Rc<Cell<usize>>,
}

impl OpQueue {
    /// New queue handle (clones share state).
    pub fn new() -> Self {
        OpQueue { executed: Rc::new(Cell::new(0)) }
    }
    /// `-addOperationWithBlock:`: runs `f` immediately.
    pub fn add_operation_with_block(&self, f: impl FnOnce() + 'static) {
        let mut op = BlockOperation::new();
        op.add_execution_block(Box::new(f));
        self.add_operation(op);
    }
    /// `-addOperation:`: runs the operation's `main` immediately.
    pub fn add_operation(&self, op: BlockOperation) {
        op.main();
        self.executed.set(self.executed.get() + 1);
    }
    /// `-waitUntilAllOperationsAreFinished`: no-op, everything already ran.
    pub fn wait_until_all_operations_are_finished(&self) {}
    /// Number of operations run so far (test aid).
    pub fn executed_count(&self) -> usize {
        self.executed.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn runs_synchronously_in_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let q = OpQueue::new();
        for i in 0..3 {
            let l = log.clone();
            q.add_operation_with_block(move || l.borrow_mut().push(i));
            assert_eq!(log.borrow().len(), i + 1);
        }
        assert_eq!(*log.borrow(), vec![0, 1, 2]);
        q.wait_until_all_operations_are_finished();
        assert_eq!(q.clone().executed_count(), 3);
    }

    #[test]
    fn block_operation_main() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut op = BlockOperation::new();
        for i in 0..3 {
            let l = log.clone();
            op.add_execution_block(Box::new(move || l.borrow_mut().push(i)));
        }
        assert_eq!(op.execution_block_count(), 3);
        op.main();
        assert_eq!(*log.borrow(), vec![0, 1, 2]);
    }
}
