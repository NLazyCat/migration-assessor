# Migrate shared file systems in an AWS large migration
<a name="migrate-shared-file-systems-in-an-aws-large-migration"></a>

*Amit Rudraraju, Sam Apa, Bheemeswararao Balla, Wally Lu, and Sanjeev Prakasam, Amazon Web Services*

## Summary
<a name="migrate-shared-file-systems-in-an-aws-large-migration-summary"></a>

Migrating 300 or more servers is considered a *large migration*. The purpose of a large migration is to migrate workloads from their existing, on-premises data centers to the AWS Cloud, and these projects typically focus on application and database workloads. However, shared file systems require focused attention and a separate migration plan. This pattern describes the migration process for shared file systems and provides best practices for migrating them successfully as part of a large migration project.

A *shared file system* (SFS), also known as a *network *or *clustered *file system, is a file share that is mounted to multiple servers. Shared file systems are accessed through protocols such as Network File System (NFS), Common Internet File System (CIFS), or Server Message Block (SMB).

These systems are not migrated with standard migration tools such as AWS Application Migration Service because they are neither dedicated to the host being migrated nor represented as a block device. Although most host dependencies are migrated transparently, the coordination and management of the dependent file systems must be handled separately.

You migrate shared file systems in the following phases: discover, plan, prepare, cut over, and validate. Using this pattern and the attached workbooks, you migrate your shared file system to an AWS storage service, such as Amazon Elastic File System (Amazon EFS), Amazon FSx for NetApp ONTAP, or Amazon FSx for Windows File Server. To transfer the file system, you can use AWS DataSync or a third-party tool, such as NetApp SnapMirror.

**Note** 
This pattern is part of an AWS Prescriptive Guidance series about [large migrations to the AWS Cloud](https://aws.amazon.com/prescriptive-guidance/large-migrations/). This pattern includes best practices and instructions for incorporating SFSs into your wave plans for servers. If you are migrating one or more shared file systems outside of a large migration project, see the data transfer instructions in the AWS documentation for [Amazon EFS](https://docs.aws.amazon.com/efs/latest/ug/trnsfr-data-using-datasync.html), [Amazon FSx for Windows File Server](https://docs.aws.amazon.com/fsx/latest/WindowsGuide/migrate-to-fsx.html), and [Amazon FSx for NetApp ONTAP](https://docs.aws.amazon.com/fsx/latest/ONTAPGuide/migrating-fsx-ontap.html).

## Prerequisites and limitations
<a name="migrate-shared-file-systems-in-an-aws-large-migration-prereqs"></a>

**Prerequisites**

Prerequisites can vary depending on your source and target shared file systems and your use case. The following are the most common:
+ An active AWS account.
+ You have completed application portfolio discovery for your large migration project and started developing wave plans. For more information, see [Portfolio playbook for AWS large migrations](https://docs.aws.amazon.com/prescriptive-guidance/latest/large-migration-portfolio-playbook/welcome.html).
+ Virtual private clouds (VPCs) and security groups that allow ingress and egress traffic between the on-premises data center and your AWS environment. For more information, see [Network-to Amazon VPC connectivity options](https://docs.aws.amazon.com/whitepapers/latest/aws-vpc-connectivity-options/network-to-amazon-vpc-connectivity-options.html) and [AWS DataSync network requirements](https://docs.aws.amazon.com/datasync/latest/userguide/datasync-network.html).
+ Permissions to create AWS CloudFormation stacks or permissions to create Amazon EFS or Amazon FSx resources. For more information, see the [CloudFormation documentation](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/using-iam-template.html), [Amazon EFS documentation](https://docs.aws.amazon.com/efs/latest/ug/security-iam.html), or [Amazon FSx documentation](https://docs.aws.amazon.com/fsx/latest/WindowsGuide/security-iam.html).
+ If you’re using AWS DataSync to perform the migration, you need the following permissions:
 + Permissions for AWS DataSync to send logs to an Amazon CloudWatch Logs log group. For more information, see [Allowing DataSync to upload logs to CloudWatch log groups](https://docs.aws.amazon.com/datasync/latest/userguide/monitor-datasync.html#cloudwatchlogs).
 + Permissions to access the CloudWatch Logs log group. For more information, see [Overview of managing access permissions to your CloudWatch Logs resources](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/iam-access-control-overview-cwl.html).
 + Permissions to create agents and tasks in DataSync. For more information, see [Required IAM permissions for using AWS DataSync](https://docs.aws.amazon.com/datasync/latest/userguide/permissions-requirements.html).

**Limitations**
+ This pattern is designed to migrate SFSs as part of a large migration project. It includes best practices and instructions for incorporating SFSs into your wave plans for migrating applications. If you are migrating one or more shared file systems outside of a large migration project, see the data transfer instructions in the AWS documentation for [Amazon EFS](https://docs.aws.amazon.com/efs/latest/ug/trnsfr-data-using-datasync.html), [Amazon FSx for Windows File Server](https://docs.aws.amazon.com/fsx/latest/WindowsGuide/migrate-to-fsx.html), and [Amazon FSx for NetApp ONTAP](https://docs.aws.amazon.com/fsx/latest/ONTAPGuide/migrating-fsx-ontap.html).
+ This pattern is based on commonly used architectures, services, and migration patterns. However, large migration projects and strategies can vary between organizations. You might need to customize this solution or the provided workbooks based on your requirements.

## Architecture
<a name="migrate-shared-file-systems-in-an-aws-large-migration-architecture"></a>

**Source technology stack**

One or more of the following:
+ Linux (NFS) file server
+ Windows (SMB) file server
+ NetApp storage array
+ Dell EMC Isilon storage array

**Target technology stack**

One or more of the following:
+ Amazon Elastic File System
+ Amazon FSx for NetApp ONTAP
+ Amazon FSx for Windows File Server

**Target architecture**

![\[Architecture diagram of using AWS DataSync to migrate on-premises shared file systems to AWS.\]](http://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/images/pattern-img/a30cf791-7a8a-4f71-8927-bc61f3b332f2/images/13232433-7d33-44c8-8998-b720f33f67b3.png)

The diagram shows the following process:

1. You establish a connection between the on-premises data center and the AWS Cloud by using an AWS service such as AWS Direct Connect or AWS Site-to-Site VPN.

1. You install the DataSync agent in the on-premises data center.

1. According to your wave plan, you use DataSync to replicate data from the source shared file system to the target AWS file share.

**Migration phases**

The following image shows the phases and high-level steps for migrating an SFS in a large migration project.

![\[Discover, plan, prepare, cut over, and validate phases of migrating shared file systems to AWS.\]](http://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/images/pattern-img/a30cf791-7a8a-4f71-8927-bc61f3b332f2/images/f1e0c94d-0eea-46a8-bdec-3297b34c1d43.png)

The [Epics](#migrate-shared-file-systems-in-an-aws-large-migration-epics) section of this pattern contains detailed instructions for how to complete the migration and use the attached workbooks. The following is a high-level overview of the steps in this phased approach.

| 
| 
| Phase | Steps | 
| --- |--- |
| Discover | 1. Using a discovery tool, you collect data about the shared file system, including servers, mount points, and IP addresses.2. Using a configuration management database (CMDB) or your migration tool, you collect details about the server, including information about the migration wave, environment, application owner, IT service management (ITSM) service name, organizational unit, and application ID. | 
| Plan | 3. Using the collected information about the SFSs and the servers, create the SFS wave plan.4. Using the information in the build worksheet, for each SFS, choose a target AWS service and a migration tool. | 
| Prepare | 5. Set up the target infrastructure in Amazon EFS, Amazon FSx for NetApp ONTAP, or Amazon FSx for Windows File Server.6. Set up the data transfer service, such as DataSync, and then start the initial data sync. When the initial sync is complete, you can set up reoccurring syncs to run on a schedule,7. Update the SFS wave plan with information about the target file share, such as the IP address or path. | 
| Cut over | 8. Stop applications that actively access the source SFS.9. In the data transfer service, perform a final data sync.10. When the sync is complete, validate that it was completely successfully by reviewing the log data in CloudWatch Logs. | 
| Validate | 11. On the servers, change the mount point to the new SFS path.12. Restart and validate the applications. | 

## Tools
<a name="migrate-shared-file-systems-in-an-aws-large-migration-tools"></a>

**AWS services**
+ [Amazon CloudWatch Logs](https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html) helps you centralize the logs from all your systems, applications, and AWS services so you can monitor them and archive them securely.
+ [AWS DataSync](https://docs.aws.amazon.com/datasync/latest/userguide/what-is-datasync.html) is an online data transfer and discovery service that helps you move files or object data to, from, and between AWS storage services.
+ [Amazon Elastic File System (Amazon EFS)](https://docs.aws.amazon.com/efs/latest/ug/whatisefs.html) helps you create and configure shared file systems in the AWS Cloud.
+ [Amazon FSx](https://docs.aws.amazon.com/fsx/?id=docs_gateway) provides file systems that support industry-standard connectivity protocols and offer high availability and replication across AWS Regions.

**Other tools**
+ [SnapMirror](https://library.netapp.com/ecmdocs/ECMP1196991/html/GUID-BA1081BE-B2BB-4C6E-8A82-FB0F87AC514E.html) is a NetApp data replication tool that replicates data from specified source volumes or [qtrees](https://library.netapp.com/ecmdocs/ECMP1154894/html/GUID-8F084F85-2AB8-4622-B4F3-2D9E68559292.html) to target volumes or qtrees, respectively. You can use this tool to migrate a NetApp source file system to Amazon FSx for NetApp ONTAP.
+ [Robocopy](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/robocopy), which is short for *Robust File Copy*, is a command-line directory and command for Windows. You can use this tool to migrate a Windows source file system to Amazon FSx for Windows File Server.

## Best practices
<a name="migrate-shared-file-systems-in-an-aws-large-migration-best-practices"></a>

**Wave planning approaches**

When planning waves for your large migration project, consider latency and application performance. When the SFS and dependent applications are operating in different locations, such as one in the cloud and one in the on-premises data center, this can increase latency and affect application performance. The following are the available options when creating wave plans:

1. **Migrate the SFS and all dependent servers within the same wave** – This approach prevents performance issues and minimizes rework, such as reconfiguring mount points multiple times. It is recommended when very low latency is required between the application and the SFS. However, wave planning is complex, and the goal is typically to remove variables from dependency groupings, not add to them. In addition, this approach isn’t recommended if many servers access the same SFS because it makes the wave too large.

1. **Migrate the SFS after the last dependent server has been migrated **– For example, if an SFS is accessed by multiple servers and those servers are scheduled to migrate in waves 4, 6, and 7, schedule the SFS to migrate in wave 7.

 This approach is often the most logical for large migrations and is recommended for latency-sensitive applications. It reduces costs associated with data transfer. It also minimizes the period of latency between the SFS and higher-tier (such as production) applications because higher-tier applications are typically scheduled to migrate last, after development and QA applications.

 However, this approach still requires discovery, planning, and agility. You might need to migrate the SFS in an earlier wave. Confirm that the applications can withstand the additional latency for the period of time between the first dependent wave and the wave containing the SFS. Conduct a discovery session with the application owners and migrate the application in same wave the most latency-sensitive application. If performance issues are discovered after migrating a dependent application, be prepared to pivot quickly to migrate the SFS as quickly as possible.

1. **Migrate the SFS at the end of the large migration project **– This approach is recommended if latency is not a factor, such as when the data in the SFS is infrequently accessed or not critical to application performance. This approach streamlines the migration and simplifies cutover tasks.

You can blend these approaches based on the latency-sensitivity of the application. For example, you can migrate latency-sensitive SFSs by using approaches 1 or 2 and then migrate the rest of the SFSs by using approach 3.

**Choosing an AWS file system service**

AWS offers several cloud services for file storage. Each offers different benefits and limitations for performance, scale, accessibility, integration, compliance, and cost optimization. There are some logical default options. For example, if your current on-premises file system is operating Windows Server, then Amazon FSx for Windows File Server is the default choice. Or if the on-premises file system is operating NetApp ONTAP, then Amazon FSx for NetApp ONTAP is the default choice. However, you might choose a target service based on the requirements of your application or to realize other cloud operating benefits. For more information, see [Choosing the right AWS file storage service for your deployment](https://d1.awsstatic.com/events/Summits/awsnycsummit/Choosing_the_right_AWS_file_storage_service_for_your_deployment_STG302.pdf) (AWS Summit presentation).

**Choosing a migration tool**

Amazon EFS and Amazon FSx support use of AWS DataSync to migrate shared file systems to the AWS Cloud. For more information about supported storage systems and services, benefits, and use cases, see [What is AWS DataSync](https://docs.aws.amazon.com/datasync/latest/userguide/what-is-datasync.html). For an overview of the process of using DataSync to transfer your files, see [How AWS DataSync transfers work](https://docs.aws.amazon.com/datasync/latest/userguide/how-datasync-transfer-works.html).

There are also several third-party tools that are available, including the following:
+ If you choose Amazon FSx for NetApp ONTAP, you can use NetApp SnapMirror to migrate the files from the on-premises data center to the cloud. SnapMirror uses block-level replication, which can be faster than DataSync and reduce the duration of the data transfer process. For more information, see [Migrating to FSx for ONTAP using NetApp SnapMirror](https://docs.aws.amazon.com/fsx/latest/ONTAPGuide/migrating-fsx-ontap-snapmirror.html).
+ If you choose Amazon FSx for Windows File Server, you can use Robocopy to migrate files to the cloud. For more information, see [Migrating existing files to FSx for Windows File Server using Robocopy](https://docs.aws.amazon.com/fsx/latest/WindowsGuide/migrate-files-to-fsx.html).

## Epics
<a name="migrate-shared-file-systems-in-an-aws-large-migration-epics"></a>

### Discover
<a name="discover"></a>

| Task | Description | Skills required | 
| --- | --- | --- | 
| Prepare the SFS discovery workbook. | [\[See the AWS documentation website for more details\]](http://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/migrate-shared-file-systems-in-an-aws-large-migration.html) | Migration engineer, Migration lead | 
| Collect information about the source SFS. | [\[See the AWS documentation website for more details\]](http://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/migrate-shared-file-systems-in-an-aws-large-migration.html) | Migration engineer, Migration lead | 
| Collect information about the servers. | [\[See the AWS documentation website for more details\]](http://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/migrate-shared-file-systems-in-an-aws-large-migration.html) | Migration engineer, Migration lead | 

### Plan
<a name="plan"></a>

| Task | Description | Skills required | 
| --- | --- | --- | 
| Build the SFS wave plan. | [\[See the AWS documentation website for more details\]](http://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/migrate-shared-file-systems-in-an-aws-large-migration.html) | Build lead, Cutover lead, Migration engineer, Migration lead | 
| Choose the target AWS service and migration tool. | [\[See the AWS documentation website for more details\]](http://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/migrate-shared-file-systems-in-an-aws-large-migration.html) | Migration engineer, Migration lead | 

### Prepare
<a name="prepare"></a>

| Task | Description | Skills required | 
| --- | --- | --- | 
| Set up the target file system. | According to the details recorded in your wave plan, set up the target file systems in the target AWS account, VPC, and subnets. For instructions, see the following AWS documentation:[\[See the AWS documentation website for more details\]](http://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/migrate-shared-file-systems-in-an-aws-large-migration.html) | Migration engineer, Migration lead, AWS administrator | 
| Set up the migration tool and transfer data. | [\[See the AWS documentation website for more details\]](http://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/migrate-shared-file-systems-in-an-aws-large-migration.html) | AWS administrator,
