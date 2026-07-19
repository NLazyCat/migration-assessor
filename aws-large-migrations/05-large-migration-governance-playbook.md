## AWS Prescriptive Guidance

### Project governance playbook for AWS large migrations

- [Introduction](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/welcome.html)

- [About managing a large migration](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/managing-large-migration.html)

- [Stage 1: Initializing](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/stage1.html)

    - [Before you begin](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/before-you-begin.html)

    - [Task: Kicking off the migrate phase](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/task-kickoff.html)

    - [Task: Defining project management processes and tools](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/task-project-management.html)

- [Stage 2: Implementing](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/stage2.html)

- [Resources](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/additional-resources.html)

- [Contributors](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/contributors.html)

- [Document history](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/doc-history.html)

1. [Documentation](https://docs.aws.amazon.com/index.html)

2. ...

3. [AWS Prescriptive Guidance](https://aws.amazon.com/prescriptive-guidance/)

4. Project governance playbook for AWS large migrations

1. [Documentation](https://docs.aws.amazon.com/index.html)

2. [AWS Prescriptive Guidance](https://aws.amazon.com/prescriptive-guidance/)

3. Project governance playbook for AWS large migrations

# 
Project governance playbook for AWS large migrations

[ PDF](https://docs.aws.amazon.com/pdfs/prescriptive-guidance/latest/large-migration-governance-playbook/large-migration-governance-playbook.pdf#welcome)

[ RSS](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/large-migration-governance-playbook.rss)

[ Markdown](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/welcome.md)

*Amazon Web Services* ([contributors](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/contributors.html))

*February 2022* ([document  history](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/doc-history.html))

###### Note

The project teams, roles, and workstreams referenced in this guide are described in the  [Foundation playbook for AWS large migrations](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-foundation-playbook/). We recommend completing the foundation  playbook in advance of starting the project governance tasks in this guide.

Effective project governance is critical to the success of a large migration to the  AWS Cloud. *Project governance* defines the rules, boundaries, and plans  for completing the migration. Common project governance tools include a communication plan,  benefit-tracking office, escalation plan, and quality gates for migration and cutover. By  completing this playbook, you create and customize the governance that defines how to run your  migration project.

In the third phase of a large migration, *migrate and modernize*, you  refine your project governance model and create many of the tools and templates that you use  during the migration. You should complete the assess and mobilize phases prior to starting this  process. For more information about the phases of a large migration, see [Phases of a large migration](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-guide/phases.html) in the  *Guide for AWS large migrations*.

This playbook provides a step-by-step approach to quickly develop an effective governance  model for a large migration project. It describes project governance for a large migration,  which spans both stages of the migrate phase, initialization and implementation:

- In stage 1, *initialize*, you assess team readiness and stand up  the governance model. You define the processes and tools that govern your large migration  project. At the end of stage 1, you have project governance tools that are customized  for your own use case. 

- In stage 2, *implement*, you use the tools you created in the  previous stage in order to adhere to your project governance plan.

## Guidance for large migrations

Migrating 300 or more servers is considered a large migration. The people, process,  and technology challenges of a large migration project are typically new to most enterprises.  This document is part of an AWS Prescriptive Guidance series about large migrations to the AWS Cloud. This  series is designed to help you apply the correct strategy and best practices from the outset,  to streamline your journey to the cloud.

The following figure shows the other documents in this series. Review the strategy first,  then the guides, and then proceed to the playbooks. To access the complete series, see [Large migrations to the  AWS Cloud](https://aws.amazon.com/prescriptive-guidance/large-migrations/).

![](https://aka.doubaocdn.com/s/KCF91wnrTw)

## About the tools and templates

In this playbook, you create the following tools. You use these tools to communicate with  the project stakeholders, including the migration teams, application owners, project sponsors,  and executive leadership. The goal of the following tools is to maximize transparency for all  project activities, which helps to accelerate the large migration:

- Kickoff presentation

- Meeting plan, including types and cadence

- Escalation plan

- Weekly project status report

- Wave workshop

- Cutover readiness assessment presentation

- Steering committee status report

- Benefit-tracking office

- Project summary dashboard

- Financial reporting process

- Resource plan

- Decision log

- Risks, actions, issues, and dependencies (RAID) log

- Communication plan and templates, such as gate communications and reminders

We recommend using the [project governance playbook templates](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/samples/project-governance-playbook-templates.zip) included in this playbook and then  customizing them for your portfolio, processes, and environment. The templates are designed to  foster effective communication, set clear expectations, and align executive leadership,  application owners, and migration project stakeholders. The instructions in this playbook  provide context as to the purpose of each of these templates, which your team can customize.  This playbook includes the following templates:

- Cutover readiness assessment template – This  template helps you track the progress of each wave through the quality gates and key  project management milestones.

- Financial glide path template – This template is  used to review financials with your project sponsors on a regular cadence.

- Kickoff presentation template – You use this  presentation template at a kickoff meeting early in stage 1.

- Meeting plan template – You use this template to  define the types of recurring meetings, establish their cadence, and identify the key  participants.

- Status report template – You use this template  in order to create a standard presentation format for the project status review  meetings.

- Steering committee meeting template – You use  this template in order to create a standard presentation format for the steering committee  meetings.

- Gate communication templates – You use these  email communication templates to share the status of the wave with project stakeholders  and inform them of recent changes or upcoming activities. This playbook includes the  following templates:

    - Communication template for cutover complete

    - Communication template for hypercare complete

    - Communication template for T-0

    - Communication template for T-1

    - Communication template for T-7

    - Communication template for T-14

    - Communication template for T-21

    - Communication template for T-28

### View related pages

Abstracts generated by AI

- 1

- 2

- 3

- 4

Prescriptive-guidance › large-migration-foundation-playbook

[Foundation playbook for AWS large migrations![e5c312859be4e5e4-0e6f56d08ae56f9f](https://prod.us-west-2.tcx-beacon.docs.aws.dev/recommendation-beacon/similar/impressions/dce8b1fd-b8b1-482a-8677-3381828f329c/tP3TdaJccnVDI6jCoZ9OEr9UZ3ZN24nlNHE4Q-q47TlidgIU0JOkTg==/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Cwelcome.html/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-foundation-playbook%7Cwelcome.html)](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-foundation-playbook/welcome.html)

*June 26, 2026*

Prescriptive-guidance › large-migration-portfolio-playbook

[Portfolio playbook for AWS large migrations![84c485de440cc28b-6793b8aba43495a5](https://prod.us-west-2.tcx-beacon.docs.aws.dev/recommendation-beacon/similar/impressions/dce8b1fd-b8b1-482a-8677-3381828f329c/tP3TdaJccnVDI6jCoZ9OEr9UZ3ZN24nlNHE4Q-q47TlidgIU0JOkTg==/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Cwelcome.html/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-portfolio-playbook%7Cwelcome.html)](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/welcome.html)

*June 26, 2026*

Prescriptive-guidance › large-migration-governance-playbook

[Task: Defining communication gates and schedules![2274bae1961db448-a8407baaba04c2ec](https://prod.us-west-2.tcx-beacon.docs.aws.dev/recommendation-beacon/similar/impressions/dce8b1fd-b8b1-482a-8677-3381828f329c/tP3TdaJccnVDI6jCoZ9OEr9UZ3ZN24nlNHE4Q-q47TlidgIU0JOkTg==/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Cwelcome.html/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Ctask-create-communication-gates.html)](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/task-create-communication-gates.html)

*June 26, 2026*

### Discover highly rated pages

Abstracts generated by AI

- 1

- 2

- 3

- 4

Prescriptive-guidance › security-reference-architecture

[The AWS Security Reference Architecture![4778ba9fc150b1f3-eec91208b6a2fe1a](https://prod.us-west-2.tcx-beacon.docs.aws.dev/recommendation-beacon/highlyRated/impressions/dce8b1fd-b8b1-482a-8677-3381828f329c/tP3TdaJccnVDI6jCoZ9OEr9UZ3ZN24nlNHE4Q-q47TlidgIU0JOkTg==/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Cwelcome.html/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Csecurity-reference-architecture%7Carchitecture.html)](https://docs.aws.amazon.com/prescriptive-guidance/latest/security-reference-architecture/architecture.html)

*June 27, 2026*

Prescriptive-guidance › architectural-decision-records

[ADR process![39ec8e4ef86eb923-d712c268ea5b489b](https://prod.us-west-2.tcx-beacon.docs.aws.dev/recommendation-beacon/highlyRated/impressions/dce8b1fd-b8b1-482a-8677-3381828f329c/tP3TdaJccnVDI6jCoZ9OEr9UZ3ZN24nlNHE4Q-q47TlidgIU0JOkTg==/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Cwelcome.html/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Carchitectural-decision-records%7Cadr-process.html)](https://docs.aws.amazon.com/prescriptive-guidance/latest/architectural-decision-records/adr-process.html)

*June 27, 2026*

Prescriptive-guidance › backup-recovery

[Amazon EC2 backup and recovery with snapshots and AMIs![4c7eb0ae4ba71f6f-c41b253c2cbb987a](https://prod.us-west-2.tcx-beacon.docs.aws.dev/recommendation-beacon/highlyRated/impressions/dce8b1fd-b8b1-482a-8677-3381828f329c/tP3TdaJccnVDI6jCoZ9OEr9UZ3ZN24nlNHE4Q-q47TlidgIU0JOkTg==/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Cwelcome.html/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Cbackup-recovery%7Cec2-backup.html)](https://docs.aws.amazon.com/prescriptive-guidance/latest/backup-recovery/ec2-backup.html)

*June 27, 2026*

- ### On this page

    1. [Guidance for large migrations](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/welcome.html#guidance-large-migrations)

- ### How to
[Create and customize migration runbooks![f3b6e77b9999e4a7-8765b4853b607875](https://prod.us-west-2.tcx-beacon.docs.aws.dev/recommendation-beacon/journey/impressions/dce8b1fd-b8b1-482a-8677-3381828f329c/tP3TdaJccnVDI6jCoZ9OEr9UZ3ZN24nlNHE4Q-q47TlidgIU0JOkTg==/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Cwelcome.html/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-migration-playbook%7Cwelcome.html)](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-migration-playbook/welcome.html)
[Implement a phased approach for large migrations to AWS![32460fdb57dc4044-b6dd5bb7b2efc773](https://prod.us-west-2.tcx-beacon.docs.aws.dev/recommendation-beacon/journey/impressions/dce8b1fd-b8b1-482a-8677-3381828f329c/tP3TdaJccnVDI6jCoZ9OEr9UZ3ZN24nlNHE4Q-q47TlidgIU0JOkTg==/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Cwelcome.html/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-guide%7Cwelcome.html)](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-guide/welcome.html)
[Coordinate and automate large-scale migrations to AWS Cloud![416c4246314c9068-cfab82d3778f54d1](https://prod.us-west-2.tcx-beacon.docs.aws.dev/recommendation-beacon/journey/impressions/dce8b1fd-b8b1-482a-8677-3381828f329c/tP3TdaJccnVDI6jCoZ9OEr9UZ3ZN24nlNHE4Q-q47TlidgIU0JOkTg==/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Cwelcome.html/https:%7C%7Cdocs.aws.amazon.com%7Csolutions%7Clatest%7Ccloud-migration-factory-on-aws%7Csolution-overview.html)](https://docs.aws.amazon.com/solutions/latest/cloud-migration-factory-on-aws/solution-overview.html)


### Learn about
[Understand portfolio assessment for large migrations![921f41ff166f7dd3-2c35cc6fb854eae6](https://prod.us-west-2.tcx-beacon.docs.aws.dev/recommendation-beacon/journey/impressions/dce8b1fd-b8b1-482a-8677-3381828f329c/tP3TdaJccnVDI6jCoZ9OEr9UZ3ZN24nlNHE4Q-q47TlidgIU0JOkTg==/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-governance-playbook%7Cwelcome.html/https:%7C%7Cdocs.aws.amazon.com%7Cprescriptive-guidance%7Clatest%7Clarge-migration-portfolio-playbook%7Cwelcome.html)](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/welcome.html)

#### Next topic:

[About managing a large migration](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-governance-playbook/managing-large-migration.html)