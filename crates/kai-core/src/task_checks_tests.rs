//! Task checks on the bench's own prompts (`bench/tasks.yaml`) and on
//! phrasings that must add no check.

use super::*;

fn labels(task: &str) -> Vec<String> {
    derive(task)
        .into_iter()
        .map(|(_, _, p)| p.label())
        .collect()
}

#[test]
fn bench_prompts_name_their_outputs() {
    let cases: &[(&str, &[&str])] = &[
        (
            "The page http://127.0.0.1:8765/prices.html lists kitchen products. What does the Blue Kettle cost? Write just the price into kai-bench/price.txt in my home folder.",
            &["file_not_empty(~/kai-bench/price.txt)"],
        ),
        (
            "Create a file called notes-42.txt in my home folder that says \"hi\".",
            &["file_not_empty(~/notes-42.txt)"],
        ),
        (
            "Which file in kai-bench/big in my home folder is the largest? Write only its file name into kai-bench/largest.txt.",
            &["file_not_empty(~/kai-bench/largest.txt)"],
        ),
        (
            "Compress the folder kai-bench/photos in my home folder into kai-bench/photos.zip so I can email it.",
            &["file_not_empty(~/kai-bench/photos.zip)"],
        ),
        (
            "Extract kai-bench/backup.tar.gz in my home folder into a new folder kai-bench/restored.",
            &["file_not_empty(~/kai-bench/restored)"],
        ),
        (
            "Open the text editor (Mousepad), write \"Shopping list: eggs\" and save it as kai-bench/shopping.txt in my home folder.",
            &["file_not_empty(~/kai-bench/shopping.txt)"],
        ),
        (
            "Make a copy of kai-bench/photo.jpg (in my home folder) that is 800 pixels wide, keeping the proportions, and save it as kai-bench/photo-small.jpg.",
            &["file_not_empty(~/kai-bench/photo-small.jpg)"],
        ),
        (
            "How much free disk space do I have on my home drive? Write just the number in GB to kai-bench/answer.txt in my home folder.",
            &["file_not_empty(~/kai-bench/answer.txt)"],
        ),
        (
            "Run the script kai-bench/jobs/build.sh in my home folder and save everything it prints into kai-bench/jobs/build.log.",
            &["file_not_empty(~/kai-bench/jobs/build.log)"],
        ),
        (
            "Export the chart as report.pdf on my Desktop.",
            &["file_not_empty(~/Desktop/report.pdf)"],
        ),
        (
            "Save the page as /tmp/out/page.txt.",
            &["file_not_empty(/tmp/out/page.txt)"],
        ),
    ];
    for (task, want) in cases {
        assert_eq!(labels(task), *want, "{task}");
    }
}

#[test]
fn show_in_the_file_manager_wants_that_window() {
    let got =
        labels("Show me the kai-bench/photos folder (in my home folder) in the file manager.");
    assert_eq!(got, ["window_title(photos)"]);
    assert_eq!(
        labels("Open my Downloads folder in the file manager"),
        ["window_title(Downloads)"]
    );
    // A folder named in a regex-looking way is matched literally.
    assert_eq!(
        labels("show the a.b+ folder in file explorer"),
        [r"window_title(a\.b\+)"]
    );
}

/// No check unless the words are unambiguous: a misread task must never
/// make the gate refuse a correct `done`.
#[test]
fn ambiguous_or_input_only_tasks_add_nothing() {
    for task in [
        "Search the web for \"Gemma 4 model card\" and open the result from Google.",
        "Open http://127.0.0.1:8765/contact.html in the browser and send the contact form.",
        "Download the invoice from http://127.0.0.1:8765/files/invoice-2026-03.txt into my Downloads folder.",
        "kai-bench/sales.csv has an Amount column. Add up the amounts.",
        "In ~/projects, save the output as notes.txt",
        "write the total into kai-bench/total.txt",
        "Go to example.com/about.html",
        "Send the report to bob@example.com",
        "Show the folder in the file manager",
        "Show me the photos folder",
        "Tidy up kai-bench/mess - move every PDF into a subfolder called PDFs.",
        "Is my firewall turned on?",
    ] {
        assert!(labels(task).is_empty(), "{task}: {:?}", labels(task));
    }
}

#[test]
fn one_check_per_path() {
    let got = labels("Save it as ~/a.txt, then copy it to ~/a.txt again and also to ~/b.txt");
    assert_eq!(got, ["file_not_empty(~/a.txt)", "file_not_empty(~/b.txt)"]);
}
