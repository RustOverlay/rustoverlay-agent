//! .NET's `List<T>.Sort(Comparison<T>)`, reproduced exactly.
//!
//! Gotcha: that sort is an unstable introsort, and the matcher's rankings depend on how it orders
//! exact ties — two faces with identical pixels, or `door.key` against `apartment.master_key`. A
//! stable `sort_by` would name a different leader on those ties, so the port keeps the original.

use std::cmp::Ordering;

const INTROSORT_SIZE_THRESHOLD: usize = 16;

/// `double.CompareTo`: NaN sorts below every number and equals itself.
pub(crate) fn compare_f64(a: f64, b: f64) -> Ordering {
    match a.partial_cmp(&b) {
        Some(order) => order,
        None => a.is_nan().cmp(&b.is_nan()).reverse(),
    }
}

/// Sorts in place in exactly the order `ArraySortHelper<T>.IntrospectiveSort` would leave it.
pub(crate) fn sort_by<T: Copy>(keys: &mut [T], compare: impl Fn(&T, &T) -> Ordering) {
    if keys.len() > 1 {
        let depth = 2 * (keys.len().ilog2() as i32 + 1);
        intro_sort(keys, depth, &compare);
    }
}

fn intro_sort<T: Copy>(keys: &mut [T], mut depth_limit: i32, compare: &impl Fn(&T, &T) -> Ordering) {
    let mut size = keys.len();

    while size > 1 {
        if size <= INTROSORT_SIZE_THRESHOLD {
            match size {
                2 => swap_if_greater(keys, compare, 0, 1),
                3 => {
                    swap_if_greater(keys, compare, 0, 1);
                    swap_if_greater(keys, compare, 0, 2);
                    swap_if_greater(keys, compare, 1, 2);
                }
                _ => insertion_sort(&mut keys[..size], compare),
            }
            return;
        }

        if depth_limit == 0 {
            heap_sort(&mut keys[..size], compare);
            return;
        }

        depth_limit -= 1;
        let pivot = pick_pivot_and_partition(&mut keys[..size], compare);
        intro_sort(&mut keys[pivot + 1..size], depth_limit, compare);
        size = pivot;
    }
}

fn swap_if_greater<T: Copy>(keys: &mut [T], compare: &impl Fn(&T, &T) -> Ordering, i: usize, j: usize) {
    if compare(&keys[i], &keys[j]) == Ordering::Greater {
        keys.swap(i, j);
    }
}

fn pick_pivot_and_partition<T: Copy>(keys: &mut [T], compare: &impl Fn(&T, &T) -> Ordering) -> usize {
    let hi = keys.len() - 1;
    let middle = hi >> 1;

    swap_if_greater(keys, compare, 0, middle);
    swap_if_greater(keys, compare, 0, hi);
    swap_if_greater(keys, compare, middle, hi);

    let pivot = keys[middle];
    keys.swap(middle, hi - 1);

    let (mut left, mut right) = (0, hi - 1);
    while left < right {
        left += 1;
        while compare(&keys[left], &pivot) == Ordering::Less {
            left += 1;
        }
        right -= 1;
        while compare(&pivot, &keys[right]) == Ordering::Less {
            right -= 1;
        }
        if left >= right {
            break;
        }
        keys.swap(left, right);
    }

    if left != hi - 1 {
        keys.swap(left, hi - 1);
    }

    left
}

fn heap_sort<T: Copy>(keys: &mut [T], compare: &impl Fn(&T, &T) -> Ordering) {
    let n = keys.len();
    for i in (1..=n >> 1).rev() {
        down_heap(keys, i, n, compare);
    }
    for i in (2..=n).rev() {
        keys.swap(0, i - 1);
        down_heap(keys, 1, i - 1, compare);
    }
}

fn down_heap<T: Copy>(keys: &mut [T], mut i: usize, n: usize, compare: &impl Fn(&T, &T) -> Ordering) {
    let d = keys[i - 1];
    while i <= n >> 1 {
        let mut child = 2 * i;
        if child < n && compare(&keys[child - 1], &keys[child]) == Ordering::Less {
            child += 1;
        }
        if compare(&d, &keys[child - 1]) != Ordering::Less {
            break;
        }
        keys[i - 1] = keys[child - 1];
        i = child;
    }
    keys[i - 1] = d;
}

fn insertion_sort<T: Copy>(keys: &mut [T], compare: &impl Fn(&T, &T) -> Ordering) {
    for i in 0..keys.len() - 1 {
        let t = keys[i + 1];
        let mut j = i as isize;
        while j >= 0 && compare(&t, &keys[j as usize]) == Ordering::Less {
            keys[j as usize + 1] = keys[j as usize];
            j -= 1;
        }
        keys[(j + 1) as usize] = t;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_large_inputs_like_any_correct_sort() {
        let mut seed = 7u32;
        let mut values: Vec<f64> = (0..5000)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                f64::from(seed >> 20)
            })
            .collect();

        sort_by(&mut values, |a, b| compare_f64(*b, *a));

        assert!(values.windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn small_inputs_are_insertion_sorted_and_so_stable() {
        let mut pairs = [(1, 'a'), (0, 'b'), (1, 'c'), (0, 'd')];
        sort_by(&mut pairs, |a, b| a.0.cmp(&b.0));
        assert_eq!(pairs, [(0, 'b'), (0, 'd'), (1, 'a'), (1, 'c')]);
    }

    #[test]
    fn nan_sorts_below_every_number() {
        assert_eq!(compare_f64(f64::NAN, -1e300), Ordering::Less);
        assert_eq!(compare_f64(f64::NAN, f64::NAN), Ordering::Equal);
    }
}
