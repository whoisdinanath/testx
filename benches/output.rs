//! Benchmarks for output formatting.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::time::Duration;
use testx::adapters::{TestCase, TestRunResult, TestStatus, TestSuite};

fn make_result(suites: usize, tests_per_suite: usize) -> TestRunResult {
    let mut suites_vec = Vec::new();
    for s in 0..suites {
        let mut tests = Vec::new();
        for t in 0..tests_per_suite {
            tests.push(TestCase {
                name: format!("test_{t}"),
                status: if t % 10 == 0 {
                    TestStatus::Failed
                } else if t % 7 == 0 {
                    TestStatus::Skipped
                } else {
                    TestStatus::Passed
                },
                duration: Duration::from_millis(10 + (t as u64 * 3)),
                error: if t % 10 == 0 {
                    Some(testx::adapters::TestError {
                        message: format!("assertion failed at line {t}"),
                        location: Some(format!("src/test.rs:{t}")),
                    })
                } else {
                    None
                },
            });
        }
        suites_vec.push(TestSuite {
            name: format!("suite_{s}"),
            tests,
        });
    }
    TestRunResult {
        suites: suites_vec,
        duration: Duration::from_secs(5),
        raw_exit_code: 1,
    }
}

fn bench_json_output(c: &mut Criterion) {
    let small = make_result(1, 10);
    let medium = make_result(5, 50);
    let large = make_result(10, 200);

    let mut group = c.benchmark_group("json_output");
    group.bench_function("small", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&small)).unwrap();
            black_box(json);
        });
    });
    group.bench_function("medium", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&medium)).unwrap();
            black_box(json);
        });
    });
    group.bench_function("large", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&large)).unwrap();
            black_box(json);
        });
    });
    group.finish();
}

fn bench_junit_xml_output(c: &mut Criterion) {
    let result = make_result(5, 100);

    c.bench_function("junit_xml_500_tests", |b| {
        b.iter(|| {
            let mut output = String::with_capacity(4096);
            output.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
            output.push_str("<testsuites>\n");
            for suite in &result.suites {
                output.push_str(&format!("  <testsuite name=\"{}\" tests=\"{}\">\n", suite.name, suite.tests.len()));
                for test in &suite.tests {
                    output.push_str(&format!("    <testcase name=\"{}\" time=\"{:.3}\"", test.name, test.duration.as_secs_f64()));
                    match test.status {
                        TestStatus::Failed => {
                            output.push_str(">\n");
                            if let Some(err) = &test.error {
                                output.push_str(&format!("      <failure message=\"{}\"/>\n", err.message));
                            }
                            output.push_str("    </testcase>\n");
                        }
                        TestStatus::Skipped => {
                            output.push_str(">\n      <skipped/>\n    </testcase>\n");
                        }
                        TestStatus::Passed => {
                            output.push_str("/>\n");
                        }
                    }
                }
                output.push_str("  </testsuite>\n");
            }
            output.push_str("</testsuites>\n");
            black_box(output);
        });
    });
}

criterion_group!(benches, bench_json_output, bench_junit_xml_output);
criterion_main!(benches);
