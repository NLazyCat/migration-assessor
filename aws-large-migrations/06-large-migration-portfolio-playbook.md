## AWS Prescriptive Guidance

### Portfolio playbook for AWS large migrations

- [Stage 1: Initializing](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/initialize.html)

- [Stage 2: Implementing](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/implement.html)

- [Glossary](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/apg-gloss.html)

1. [Documentation](https://docs.aws.amazon.com/index.html)

2. [AWS Prescriptive Guidance](https://aws.amazon.com/prescriptive-guidance/)

3. Portfolio playbook for AWS large migrations

1. [Documentation](https://docs.aws.amazon.com/index.html)

2. [AWS Prescriptive Guidance](https://aws.amazon.com/prescriptive-guidance/)

3. Portfolio playbook for AWS large migrations

# 
Portfolio playbook for AWS large migrations

[ PDF](https://docs.aws.amazon.com/pdfs/prescriptive-guidance/latest/large-migration-portfolio-playbook/large-migration-portfolio-playbook.pdf#welcome)

[ RSS](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/large-migration-portfolio-playbook.rss)

[ Markdown](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/welcome.md)

*Amazon Web Services* ([contributors](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/contributors.html))

*July 2024* ([document history](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/doc-history.html))

###### Note

Performing an initial, high-level discovery and assessment of the application portfolio is  a prerequisite to completing the tasks in this playbook. For more information about completing  this process, see the [Application portfolio assessment guide for AWS Cloud migration](https://docs.aws.amazon.com/prescriptive-guidance/latest/application-portfolio-assessment-guide/).

In a large migration, the portfolio workstream plans waves of applications for migration,  and the migration workstream focuses on migrating those waves. When planning waves, the  portfolio workstream is responsible for assessing the portfolio, collecting the metadata needed  for the migration, prioritizing the applications, and then assigning the applications to waves.  Waves must be sized and scheduled according to the capacity of the migration workstream and must  account for the complexity of the application, dependencies, and any business factors, such as  budgets, performance goals, resource availability, and deadlines. For more information about  core and supporting workstreams, see [Workstreams in a large migration](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-foundation-playbook/workstreams.html) in the  *Foundation playbook for AWS large migrations*.

This playbook provides a step-by-step approach to performing a detailed portfolio assessment  for a large migration project, including application assessment and wave planning. It describes  the tasks of the portfolio workstream, which spans both stages of a large migration,  initialization and implementation:

- In stage 1, *initialize*, you validate your initial portfolio  discovery and migration strategy, and you create runbooks that define the processes and  rules used for portfolio assessment and wave planning. At the end of stage 1, you have  portfolio runbooks and tracking tools that are customized for your own portfolio, processes,  and infrastructure. 

- In stage 2, *implement*, you use the runbooks you created in the  previous stage in order to complete the portfolio assessment and wave plans.

Detailed portfolio assessment and wave planning is not a one-off task. It is a continuous  workstream that supports the migration. In a migration factory, portfolio assessment and wave  planning provide the raw materials (servers) to the factory, so you must continue with these  activities until the migration project is complete. For more information about the migration  factory model, see the [Guide for AWS large migrations](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-guide/).

## Guidance for large migrations

Migrating 300 or more servers is considered a large migration. The people, process,  and technology challenges of a large migration project are typically new to most enterprises.  This document is part of an AWS Prescriptive Guidance series about large migrations to the AWS Cloud. This  series is designed to help you apply the correct strategy and best practices from the outset,  to streamline your journey to the cloud.

The following figure shows the other documents in this series. Review the strategy first,  then the guides, and then proceed to the playbooks. To access the complete series, see [Large migrations to the  AWS Cloud](https://aws.amazon.com/prescriptive-guidance/large-migrations/).

![](https://aka.doubaocdn.com/s/WHM41wnrTw)

## About the runbooks, tools, and templates

In this playbook, you create the following runbooks:

- Application prioritization runbook

- Metadata management runbook

- Wave planning runbook

In addition, you create the following tools, which you use for tracking progress or  documenting decisions and other important information:

- Application complexity score sheet

- Application target state worksheet

- Portfolio assessment progress tracker

- Questionnaire for application owners

- Wave planning and migration dashboard

We recommend using the [portfolio  playbook templates](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/samples/portfolio-playbook-templates.zip) and then customizing them for your portfolio, processes, and  environment. The instructions in this playbook tell you when and how to customize each of  these templates. This playbook includes the following templates:

- **Application target state worksheet**  – You use  this template to define the future state of an application in the AWS Cloud when the  application or migration strategy is particularly complex.

- **Dashboard template for wave planning and migration**  – You use this template to collate critical metadata, analyze the application  portfolio, identify dependencies, and plan the migration waves. 

- **Progress tracking template for portfolio assessment**  – You use this template to track the progress of each application through the  portfolio workstream.

- **Questionnaire template for application owners**  – You use this template in the application deep dive process in order to collect  information about the application directly from the application owners.

- **Runbook template for application prioritization**  – This template is a starting point for building your own application  prioritization and deep dive processes. 

- **Runbook template for metadata management**  –  This template is a starting point for building your own metadata identification and  collection processes.

- **Runbook template for wave planning**  – This  template is a starting point for building your own wave planning processes.

- **Score sheet template for application complexity**  – You can use this template to evaluate the complexity of migrating each  application to the cloud, and then you can use the resulting score during the application  prioritization process. 

- ### On this page

    1. [Guidance for large migrations](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/welcome.html#guidance-large-migrations)

    2. [About the runbooks, tools, and templates](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/welcome.html#about-components)

#### Next topic:

[Stage 1: Initializing](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/initialize.html)