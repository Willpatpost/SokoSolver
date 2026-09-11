/// Hungarian algorithm for minimum-cost bipartite matching.
/// Returns the minimum total cost and the assignment (row -> col).
/// cost[i][j] is the cost of assigning row i to column j.
/// Handles rectangular matrices (n_rows <= n_cols).
/// Uses u32 costs with u32::MAX as infinity.
pub fn hungarian(cost: &[Vec<u32>]) -> (u32, Vec<usize>) {
    let n_rows = cost.len();
    if n_rows == 0 {
        return (0, vec![]);
    }
    let n_cols = cost[0].len();
    assert!(
        n_rows <= n_cols,
        "must have at least as many columns as rows"
    );

    let n = n_cols;

    let mut u = vec![0i64; n_rows + 1];
    let mut v = vec![0i64; n + 1];
    let mut p = vec![0usize; n + 1];
    let mut way = vec![0usize; n + 1];

    for i in 1..=n_rows {
        p[0] = i;
        let mut j0 = 0usize;
        let mut minv = vec![i64::MAX; n + 1];
        let mut used = vec![false; n + 1];

        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = i64::MAX;
            let mut j1 = 0usize;

            for j in 1..=n {
                if used[j] {
                    continue;
                }
                let c = if i0 <= n_rows {
                    cost[i0 - 1][j - 1] as i64
                } else {
                    0
                };
                let cur = c - u[i0] - v[j];
                if cur < minv[j] {
                    minv[j] = cur;
                    way[j] = j0;
                }
                if minv[j] < delta {
                    delta = minv[j];
                    j1 = j;
                }
            }

            for j in 0..=n {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }

            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }

        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }

    let mut assignment = vec![0usize; n_rows];
    for j in 1..=n {
        if p[j] > 0 && p[j] <= n_rows {
            assignment[p[j] - 1] = j - 1;
        }
    }

    let total: u32 = assignment
        .iter()
        .enumerate()
        .map(|(i, &j)| cost[i][j])
        .sum();

    (total, assignment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_assignment() {
        let cost = vec![vec![0, 10, 10], vec![10, 0, 10], vec![10, 10, 0]];
        let (total, assign) = hungarian(&cost);
        assert_eq!(total, 0);
        assert_eq!(assign, vec![0, 1, 2]);
    }

    #[test]
    fn simple_3x3() {
        let cost = vec![vec![1, 2, 3], vec![2, 4, 6], vec![3, 6, 9]];
        let (total, _assign) = hungarian(&cost);
        assert_eq!(total, 10);
    }

    #[test]
    fn rectangular_2x3() {
        let cost = vec![vec![5, 1, 3], vec![2, 8, 7]];
        let (total, assign) = hungarian(&cost);
        assert_eq!(total, 3);
        assert_eq!(assign[0], 1);
        assert_eq!(assign[1], 0);
    }

    #[test]
    fn single_element() {
        let cost = vec![vec![42]];
        let (total, assign) = hungarian(&cost);
        assert_eq!(total, 42);
        assert_eq!(assign, vec![0]);
    }

    #[test]
    fn empty() {
        let cost: Vec<Vec<u32>> = vec![];
        let (total, assign) = hungarian(&cost);
        assert_eq!(total, 0);
        assert!(assign.is_empty());
    }

    #[test]
    fn larger_problem() {
        let cost = vec![
            vec![7, 53, 183, 439],
            vec![497, 383, 563, 79],
            vec![627, 343, 773, 959],
            vec![340, 393, 469, 611],
        ];
        let (total, _) = hungarian(&cost);
        assert!(total <= 7 + 79 + 343 + 469);
    }

    #[test]
    fn all_same_cost() {
        let cost = vec![vec![5, 5, 5], vec![5, 5, 5], vec![5, 5, 5]];
        let (total, _) = hungarian(&cost);
        assert_eq!(total, 15);
    }
}
