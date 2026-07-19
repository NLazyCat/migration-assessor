## AWS Prescriptive Guidance

### Migration playbook for AWS large migrations

1. [Documentation](https://docs.aws.amazon.com/index.html)

2. ...

3. [AWS Prescriptive Guidance](https://aws.amazon.com/prescriptive-guidance/)

4. Migration playbook for AWS large migrations

1. [Documentation](https://docs.aws.amazon.com/index.html)

2. [AWS Prescriptive Guidance](https://aws.amazon.com/prescriptive-guidance/)

3. Migration playbook for AWS large migrations

# 
Migration playbook for AWS large migrations

[ PDF](https://docs.aws.amazon.com/pdfs/prescriptive-guidance/latest/large-migration-migration-playbook/large-migration-migration-playbook.pdf#welcome)

[ RSS](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-migration-playbook/large-migration-migration-playbook.rss)

[ Markdown](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-migration-playbook/welcome.md)

*Amazon Web Services* ([contributors](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-migration-playbook/contributors.html))

*February 2022* ([document  history](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-migration-playbook/doc-history.html))

In a large migration, the migration workstream uses the wave plans and migration metadata  supplied by the portfolio workstream in order to migrate workloads to the AWS Cloud. The  migration workstream is responsible for submitting any change requests, migrating the  application, coordinating application testing with the application owners, performing cutover,  and monitoring the application through the hypercare period. In the first stage, initializing a  large migration, you create the runbooks that the migration workstream uses to migrate the  applications and servers. In the second stage, implementing a large migration, the migration  workstream plans sprints and uses the migration runbooks in order to migrate and cutover the  applications. For more information about core and supporting workstreams, see [Workstreams in a large migration](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-foundation-playbook/workstreams.html) in the  *Foundation playbook for AWS large migrations*.

This migration playbook outlines the tasks of the migration workstream, which spans both  stages of a large migration, initialization and implementation:

- In stage 1, *initialize*, you draft, test, and refine the  runbooks, and then you automate manual tasks for each migration pattern.

- In stage 2, *implement*, you perform the migration with the  predefined runbooks built in stage 1.

## Guidance for large migrations

Migrating 300 or more servers is considered a large migration. The people, process,  and technology challenges of a large migration project are typically new to most enterprises.  This document is part of an AWS Prescriptive Guidance series about large migrations to the AWS Cloud. This  series is designed to help you apply the correct strategy and best practices from the outset,  to streamline your journey to the cloud.

The following figure shows the other documents in this series. Review the strategy first,  then the guides, and then proceed to the playbooks. To access the complete series, see [Large migrations to the  AWS Cloud](https://aws.amazon.com/prescriptive-guidance/large-migrations/).

![](https://aka.doubaocdn.com/s/B0dN1wnrTw)

## About the runbooks, tools, and templates

We recommend using the [migration  playbook templates](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-migration-playbook/samples/migration-playbook-templates.zip) and then customizing them for your portfolio, processes, and  environment. The provided templates include standard processes, typical cutover processes, and  placeholders for processes that are unique to your environment. The instructions in this  playbook tell you when and how to customize each of these templates. This playbook includes  the following templates:

- Rehost migration runbook template

- Rehost migration task list template

For migration patterns, from which you can build your own runbooks, see [AWS Prescriptive Guidance migration patterns](https://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/migration-pattern-list.html).

Migration runbooks require varying levels of detail:

- **Detailed runbooks**  – Detailed runbooks are best  suited for migration patterns that you will repeat many times. For these patterns, we  recommend starting with the *Rehost migration runbook template*  (Microsoft Word format). This template captures as many details as possible, including  screenshots and step-by-step instructions, and it is designed to help multiple people  perform the same task consistently.

- **Task List**  – For migration patterns that are  one-off or very simple, a short task list is a better option. For these patterns, we  recommend starting with the *Rehost migration task list* template  (Microsoft Excel format). This template contains a high-level task list and is typically  used for tracking and managing ownership of tasks. You can also use a task list to track  the status of tasks that are documented in a runbook.

Whether you are using a detailed runbook or a short task list, verify that your runbook  describes the tasks in sequence. For complex tasks, you can provide links to external  documentation.

- ### On this page

    1. [Guidance for large migrations](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-migration-playbook/welcome.html#guidance-large-migrations)

    2. [About the runbooks, tools, and templates](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-migration-playbook/welcome.html#about-components)

#### Next topic:

[Stage 1: Initializing](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-migration-playbook/stage-one-initialization.html)