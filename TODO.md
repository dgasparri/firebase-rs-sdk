# Miscellaneous TODO

## Create a RUSTDOC.md with extracts from README.md


Create a slimmer, less noisy RUSTDOC.md file for each module, with sections extracted from the official module's README.md (better if through a script) to be included as a DOC in the file, less crowded that the README.md file

#![doc = include_str!("RUSTDOC.md")]