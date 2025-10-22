# Contributing to the unofficial Firebase RS SDK

This is mostly an AI generated library, so any contribution of either time to feed and check the AI's work, or of AI resources, is greatly appreciated!

## Setting up the environment

Here are the steps to set up the environment.

Clone the Github repository:

> git clone https://github.com/dgasparri/firebase-rs-sdk.git

Clone also the Github Firebase JS SDK repository:

> https://github.com/firebase/firebase-js-sdk.git

Copy all the files and subforlder from the Firebase JS SDK ./packages folder, to the ./packages folder. Those files will be used by the AI to analyze the features of the JS SDK library. Under Windows Command Line:

> XCOPY /E firebase-js-sdk\packages\* firebase-rs-sdk\packages\.

Copy also the doc files in the docs-devsite folder. Those files will be used by the AI to help document the API. Under Windows Command Line:

> COPY firebase-js-sdk\docs-devsite\* firebase-rs-sdk\docs-devsite\.

Now you can delete the JS SDK folder, as it will not needed anymore.

## Common AI prompts to develop code/documentation for this library

This library is mainly written by AI. Detailed instructions for the AI are given in the ./AGENTS.md file. Here are some of the prompts we commonly used to work on the library. It is not an extensive list of the prompts, but so far they have worked fine for us.

For implementing a specific feature you are interested in:

> Following the instructions in ./AGENTS.md, implement the feature {XXX} for the module {module}.

Example: Following the instructions in ./AGENTS.md, implement the StorageReference operations for the module storage.

The feature **must be present** in the original Firebase JS SDK, or it will not enter into this library, even if helpful. 

For moving forward the porting of a module you are interested in, leaving to the AI to decide what it should work on:

> Following the instructions in ./AGENTS.md, read the file ./src/{module}/README.md what are the next steps and the missing features in the module {module} and work on the first step

For documenting some of the API:

> Following the instructions in ./AGENTS.md, review the Rust code for the module {module} and write or improve the appropriate documentation

For creating an example of some feature you might be interested in:

> Following the instructions in ./AGENTS.md, write an example for the module {module} demonstrating how to use the feature {feature}. Save the example in the folder ./examples with a filename starting with the {module} name

For porting some of the tests from the JS SDK library:

> Following the instructions in ./AGENTS.md, review the tests in the Typescript code in ./packages/{module} and port some of the relevant tests to Rust

For a failed test:

> Follow the instructions in ./AGENTS.md. I ran `cargo test` and it failed at the test {name_of_test}. Here is the output of the test with the failure message: \[Content of the cargo test output\]

For a bug:

> Follow the instructions in ./AGENTS.md. The module {module} did not work as expected, I suspect a bug. The expected behavior of the following code is \[expected behavior\], but I obtained \[actual behavior\]. Here is the code: 
>
>```
> // Code
> ```

For updating the README.md of any module:

> Follow the instructions in ./AGENTS.md. Review the Typescript code in ./packages/{module} and the Rust code in ./scr/{module}, and check if ./src/{module}/README.md is up to date. Check specifically for the features implemented and the feature still to be implemented. Make the necessary correction to bring the file up to date.

For preparing for a PULL REQUEST:

> Follow the instructions in ./AGENTS.md. I want to make a pull request to the GitHub repository, write a title and a message explaining in detail what are the changes in the code and the benefits of those changes

For having an estimate of the porting advancement

> Compare the original JS/Typescript files in ./packages/{module} and the ported files in Rust in ./src/{module}, and give me an estimated guess, in percentage, of how much of the features/code of the Firebase JS SDK has been ported to Rust for this module

## Before any pull request

Before any pull request, the following steps must be taken:

1. you should format your code using `cargo test` 
2. you MUST ask the AI to update the ./src/{module}/README.md file. It is clearly stated in the ./AGENTS.md, but sometimes it forgets to do it. We provide a prompt for that.
3. you MUST run `cargo test` and all the tests should pass
4. you MUST compile the docs with `cargo doc` and see that there are no errors
5. you should ask the AI to write a title and a message for the pull request. You can also do it yourself, but please be specific and precise  

## Bugs and erroneous documentation

Chances are, there are bugs in the code. If you find one, or if you notice that something is not documented correctly, you can open an issue on Github or submit a Pull request.

## Before you Contribute

The code you contribute MUST be licensed under the Apache 2.0.


## Testing

In the module analytics a unit test that exercises the dispatcher is skipped by default unless FIREBASE_NETWORK_TESTS=1 is set.
