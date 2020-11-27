## Run GCS storage backend tests with GCP instead of fake-gcs

1. Export a serviceaccount key with access to bucket. This can most easily
be done on the web GUI, under `IAM/Service accounts`.
Put the exported key file (in JSON format) in the root of the project.


2. Change the constant at the begin of `gcs.rs` to point to GCP:

const GCS_SA_KEY: &'static str = "bolcom-dev-unftp-dev-738-09647c413689.json";
const GCS_BASE_URL: &'static str = "https://www.googleapis.com";
const GCS_BUCKET: &'static str = "bolcom-dev-unftp-dev-738-unftp-dev";


Make sure the `GCS_SA_KEY` points to the exported `.json` file. The path is
relative to the root of the project.

Make sure `GCS_BUCKET` points to the name of your bucket.

3. Run the tests.

4. Be careful not to accidentally commit and push your credentials. ;)
