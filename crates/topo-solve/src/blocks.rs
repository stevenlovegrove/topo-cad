//! Block vectors, and the block operators that act on them.
//!
//! The problem is held as blocks, one to a frame, and operators of a few
//! shapes between them. This follows the `robot` crate's `block_matrix`
//! (same names, same conventions), so that the two can share one
//! implementation when that module is split out of it:
//!
//! - An absent block (`None`) is zero.
//! - **Blocks are not matrices.** A block of an operator says how it
//!   multiplies, through [`BlockAct`] and [`BlockActTranspose`], and is
//!   stored however suits it (a pose is a rotation and a translation, and
//!   "multiplying" by it may be a composition). A block of a vector says how
//!   it sums, through [`BlockVector`].
//! - Frames are numbered parents first.
//! - **The topology is a matrix, not a walk of a tree.** [`ParentChild`] is
//!   unit lower triangular: the identity on its diagonal and, in each row,
//!   minus one block in the column of the parent. Solving with it is a
//!   forward substitution (each frame's quantity from its parent's), and
//!   with its transpose a back substitution (each frame's gathered into its
//!   parent's). Everything else is block diagonal, or has a row to a
//!   constraint with a block for each frame it touches.

/// The product `block * rhs`, for a block that need not be stored as a matrix.
pub trait BlockAct<Rhs> {
    type Output;
    fn act(&self, rhs: &Rhs) -> Self::Output;
}

/// The product `transpose(block) * rhs`.
pub trait BlockActTranspose<Rhs> {
    type Output;
    fn act_transpose(&self, rhs: &Rhs) -> Self::Output;
}

/// A block of a vector: these can be summed.
pub trait BlockVector: Clone {
    fn block_add(&self, rhs: &Self) -> Self;
}

/// A column of blocks, one to a frame.
pub type BlockCol<X> = Vec<Option<X>>;

fn add_assign<X: BlockVector>(block: &mut Option<X>, rhs: &X) {
    *block = Some(match block {
        Some(b) => b.block_add(rhs),
        None => rhs.clone(),
    });
}

/// The topology: `(i, i)` is the identity, `(i, parent(i))` is `-blocks[i]`,
/// and nothing else. `blocks[i]` takes a quantity of the parent of `i` to a
/// quantity of `i`. The root's block takes what is given for the world.
#[derive(Debug, Clone)]
pub struct ParentChild<B> {
    parents: Vec<Option<usize>>,
    blocks: Vec<B>,
}

impl<B> Default for ParentChild<B> {
    fn default() -> Self {
        ParentChild { parents: vec![], blocks: vec![] }
    }
}

impl<B> ParentChild<B> {
    /// One more frame, in `parent`: its number.
    pub fn push(&mut self, parent: Option<usize>, block: B) -> usize {
        assert!(parent.is_none_or(|p| p < self.blocks.len()), "parents are numbered before their children");
        self.parents.push(parent);
        self.blocks.push(block);
        self.blocks.len() - 1
    }

    pub fn blocks(&self) -> &[B] {
        &self.blocks
    }

    pub fn blocks_mut(&mut self) -> &mut [B] {
        &mut self.blocks
    }

    pub fn new(parents: Vec<Option<usize>>, blocks: Vec<B>) -> Self {
        assert_eq!(parents.len(), blocks.len());
        for (i, p) in parents.iter().enumerate() {
            assert!(p.is_none_or(|p| p < i), "parents are numbered before their children");
        }
        ParentChild { parents, blocks }
    }

    pub fn parents(&self) -> &[Option<usize>] {
        &self.parents
    }

    /// The same topology with its blocks seen through `map` (a pose seen as how it carries a motion).
    pub fn map<U>(&self, map: impl Fn(&B) -> U) -> ParentChild<U> {
        ParentChild { parents: self.parents.clone(), blocks: self.blocks.iter().map(map).collect() }
    }

    /// What `world` comes to at each frame: `x[i] = blocks[i] * x[parent(i)]`. Each is a single
    /// product and never a sum, so `X` need not be summable (it may be a pose).
    pub fn from_world<X: Clone>(&self, world: &X) -> Vec<X>
    where
        B: BlockAct<X, Output = X>,
    {
        let mut x: Vec<X> = Vec::with_capacity(self.blocks.len());
        for (i, b) in self.blocks.iter().enumerate() {
            let next = match self.parents[i] {
                Some(p) => b.act(&x[p]),
                None => b.act(world),
            };
            x.push(next);
        }
        x
    }

    /// Solves `self * x = rhs` by forward substitution: `x[i] = rhs[i] + blocks[i] * x[parent(i)]`.
    pub fn solve<X: BlockVector>(&self, rhs: &BlockCol<X>) -> BlockCol<X>
    where
        B: BlockAct<X, Output = X>,
    {
        let mut x = rhs.clone();
        for i in 0..self.blocks.len() {
            if let Some(p) = self.parents[i] {
                if let Some(carried) = x[p].as_ref().map(|xp| self.blocks[i].act(xp)) {
                    add_assign(&mut x[i], &carried);
                }
            }
        }
        x
    }

    /// Solves `transpose(self) * x = rhs` by back substitution:
    /// `x[parent(i)] += transpose(blocks[i]) * x[i]`, children before parents.
    pub fn solve_transpose<X: BlockVector>(&self, rhs: &BlockCol<X>) -> BlockCol<X>
    where
        B: BlockActTranspose<X, Output = X>,
    {
        let mut x = rhs.clone();
        for i in (0..self.blocks.len()).rev() {
            if let Some(p) = self.parents[i] {
                if let Some(gathered) = x[i].as_ref().map(|xi| self.blocks[i].act_transpose(xi)) {
                    add_assign(&mut x[p], &gathered);
                }
            }
        }
        x
    }
}

/// One block to a frame, acting on that frame alone.
#[derive(Debug, Clone)]
pub struct Diagonal<B>(pub Vec<B>);

impl<B> Diagonal<B> {
    pub fn apply<X, Y>(&self, x: &BlockCol<X>) -> BlockCol<Y>
    where
        B: BlockAct<X, Output = Option<Y>>,
    {
        self.0.iter().zip(x).map(|(b, x)| x.as_ref().and_then(|x| b.act(x))).collect()
    }

    pub fn apply_transpose<X, Y>(&self, x: &BlockCol<X>) -> BlockCol<Y>
    where
        B: BlockActTranspose<X, Output = Option<Y>>,
    {
        self.0.iter().zip(x).map(|(b, x)| x.as_ref().and_then(|x| b.act_transpose(x))).collect()
    }
}

/// A block operator with a row to each of something (a constraint) and, in
/// it, a block for each frame it touches: `(frame, block)`, the rest absent.
#[derive(Debug, Clone, Default)]
pub struct SparseRows<B>(pub Vec<Vec<(usize, B)>>);

impl<B> SparseRows<B> {
    /// `out[row] = sum of block * x[frame]`.
    pub fn apply<X, Y: BlockVector>(&self, x: &BlockCol<X>) -> BlockCol<Y>
    where
        B: BlockAct<X, Output = Y>,
    {
        self.0
            .iter()
            .map(|row| {
                let mut out = None;
                for (f, b) in row {
                    if let Some(x) = &x[*f] {
                        add_assign(&mut out, &b.act(x));
                    }
                }
                out
            })
            .collect()
    }

    /// `out[frame] = sum of transpose(block) * y[row]`, for `frames` frames.
    pub fn apply_transpose<Y, X: BlockVector>(&self, y: &BlockCol<Y>, frames: usize) -> BlockCol<X>
    where
        B: BlockActTranspose<Y, Output = X>,
    {
        let mut out: BlockCol<X> = vec![None; frames];
        for (row, y) in self.0.iter().zip(y) {
            if let Some(y) = y {
                for (f, b) in row {
                    add_assign(&mut out[*f], &b.act_transpose(y));
                }
            }
        }
        out
    }
}

impl BlockVector for f64 {
    fn block_add(&self, rhs: &f64) -> f64 {
        self + rhs
    }
}

impl BlockVector for Vec<f64> {
    fn block_add(&self, rhs: &Vec<f64>) -> Vec<f64> {
        self.iter().zip(rhs).map(|(a, b)| a + b).collect()
    }
}

impl<const N: usize> BlockVector for [f64; N] {
    fn block_add(&self, rhs: &[f64; N]) -> [f64; N] {
        std::array::from_fn(|k| self[k] + rhs[k])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block that multiplies by a number.
    #[derive(Clone)]
    struct Times(f64);
    impl BlockAct<f64> for Times {
        type Output = f64;
        fn act(&self, x: &f64) -> f64 {
            self.0 * x
        }
    }
    impl BlockActTranspose<f64> for Times {
        type Output = f64;
        fn act_transpose(&self, x: &f64) -> f64 {
            self.0 * x
        }
    }

    #[test]
    fn the_topology_solves_by_substitution_and_its_transpose_is_its_transpose() {
        // 0 is the root; 1 and 2 are its children; 3 is a child of 1.
        let m = ParentChild::new(vec![None, Some(0), Some(0), Some(1)], vec![Times(1.0), Times(2.0), Times(3.0), Times(5.0)]);
        // Each frame's quantity is its own and what its ancestors carry to it.
        let x = m.solve(&vec![Some(1.0), Some(10.0), None, Some(100.0)]);
        assert_eq!(x, vec![Some(1.0), Some(12.0), Some(3.0), Some(160.0)]);
        // <y, M^-1 x> = <M^-T y, x>.
        let (a, b): (BlockCol<f64>, BlockCol<f64>) = (vec![Some(0.3), Some(-1.0), Some(2.0), Some(0.7)], vec![Some(1.5), None, Some(-0.4), Some(2.0)]);
        let dot = |p: &BlockCol<f64>, q: &BlockCol<f64>| p.iter().zip(q).map(|(p, q)| p.unwrap_or(0.0) * q.unwrap_or(0.0)).sum::<f64>();
        assert!((dot(&b, &m.solve(&a)) - dot(&m.solve_transpose(&b), &a)).abs() < 1e-12);
        // From the world: a product down each path, with nothing summed.
        assert_eq!(m.from_world(&1.0), vec![1.0, 2.0, 3.0, 10.0]);
    }
}
