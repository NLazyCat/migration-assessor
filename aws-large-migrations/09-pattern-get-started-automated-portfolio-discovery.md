# Get started with automated portfolio discovery
<a name="get-started-with-automated-portfolio-discovery"></a>
*Pratik Chunawala and Rodolfo Jr. Cerrada, Amazon Web Services*
## Summary
<a name="get-started-with-automated-portfolio-discovery-summary"></a>
Assessing the portfolio and collecting metadata is a critical challenge when migrating applications and servers to the Amazon Web Services (AWS) Cloud, especially for large migrations that have more than 300 servers. Using an automated portfolio discovery tool can help you collect information about your applications, such as the number of users, frequency of use, dependencies, and information about the application’s infrastructure. This information is essential when planning migration waves so that you can properly prioritize and group applications with similar traits. Using a discovery tool streamlines communication between the portfolio team and the application owners because the portfolio team can validate the results of the discovery tool rather than manually collecting the metadata. This pattern discusses key considerations for selecting an automated discovery tool and information about how to deploy and test one in your environment.
This pattern includes a template, which is a starting point for building your own checklist of high-level activities. Next to the checklist is template for a responsible, accountable, consulted, informed (RACI) matrix. You can use this RACI matrix to determine who is responsible for each task in your checklist.
## Epics
<a name="get-started-with-automated-portfolio-discovery-epics"></a>
### Select a discovery tool
<a name="select-a-discovery-tool"></a>
| Task | Description | Skills required | 
| --- | --- | --- | 
| Determine whether a discovery tool is appropriate for your use case. | A discovery tool might not be the best solution for your use case. Consider the amount of time required to select, procure, prepare, and deploy a discovery tool. It can take 4–8 weeks to set up the scanning appliance for an agentless discovery tool in your environment or to install agents to all in-scope workloads. Once deployed, you must allow 4–12 weeks for the discovery tool to collect metadata by scanning the application workloads and performing application stack analysis. If you are migrating fewer than 100 servers, you might be able to manually collect the metadata and analyze dependencies faster than the time required to deploy and collect metadata with an automated discovery tool.  | Migration lead, Migration engineer |
| Select a discovery tool. | Review the **Considerations for selecting an automated discovery tool** in the [Additional information](#get-started-with-automated-portfolio-discovery-additional) section. Determine the appropriate criteria for selecting a discovery tool for your use case, and then evaluate each tool against those criteria. For a comprehensive list of automated discovery tools, see [Discovery, Planning, and Recommendation migration tools](https://aws.amazon.com/prescriptive-guidance/migration-tools/migration-discovery-tools/). | Migration lead, Migration engineer |
### Prepare for installation
<a name="prepare-for-installation"></a>
| Task | Description | Skills required | 
| --- | --- | --- | 
| Prepare the pre-deployment checklist.  | Create a checklist of the tasks you must complete before deploying the tool. For an example, see [Predeployment Checklist](https://docs.flexera.com/foundationcloudscape/ug/Content/helplibrary/FCGS_Predeployment.htm) on the Flexera documentation website. | Build lead, Migration engineer, Migration lead, Network administrator |
| Prepare the network requirements. | Provision the ports, protocols, IP addresses, and routing necessary for the tool to run and access the target servers. For more information, see the installation guide for your discovery tool. For an example, see [Deployment Requirements](https://docs.flexera.com/foundationcloudscape/help/RCDeployReq.htm) on the Flexera documentation website. | Migration engineer, Network administrator, Cloud architect |
| Prepare the account and credential requirements. | Identify the credentials you need to access the target servers and to install all of the tool’s components. | Cloud administrator, General AWS, Migration engineer, Migration lead, Network administrator, AWS administrator |
| Prepare the appliances on which you will install the tool. | Ensure that the appliances on which you will install the tool components meet the specifications and platform requirements for the tool. | Migration engineer, Migration lead, Network administrator |
| Prepare the change orders. | According to the change management process in your organization, prepare the any change orders needed, and ensure these change orders are approved. | Build lead, Migration lead |
| Send requirements to stakeholders. | Send the pre-deployment checklist and network requirements to the stakeholders. Stakeholders should review, evaluate, and prepare the necessary requirements before proceeding with the deployment. | Build lead, Migration lead |
### Deploy the tool
<a name="deploy-the-tool"></a>
| Task | Description | Skills required | 
| --- | --- | --- | 
| Download the installer. | Download the installer or the virtual machine image. Virtual machine images typically come in Open Virtualization Format (OVF). | Build lead, Migration lead |
| Extract the files. | If you are using an installer, you must download and run the installer on an on-premises server. | Build lead, Migration lead |
| Deploy the tool on the servers. | Deploy the discovery tool on the target, on-premises servers as follows:[See the AWS documentation website for more details](http://docs.aws.amazon.com/prescriptive-guidance/latest/patterns/get-started-with-automated-portfolio-discovery.html) | Build lead, Migration lead, Network administrator |
| Log in to the discovery tool. | Follow the on-screen prompts, and log in to get started with the tool. | Migration lead, Build lead |
| Activate the product. | Enter your license key. | Build lead, Migration lead |
| Configure the tool. | Enter any credentials necessary to access the target servers, such as credentials for Windows, VMware, Simple Network Management Protocol (SNMP), and Secure Shell Protocol (SSH), or databases. | Build lead, Migration lead |
### Test the tool
<a name="test-the-tool"></a>
| Task | Description | Skills required | 
| --- | --- | --- | 
| Select test servers. | Identify a small set of non-production subnets or IP addresses that you can use to test the discovery tool. This helps you validate the scans quickly, identify and troubleshoot any errors quickly, and isolate your tests from production environments. | Build lead, Migration lead, Network administrator |
| Start scanning the selected test servers. | For an agentless discovery tool, enter the subnets or IP addresses for the selected test servers in the discovery tool console, and start the scan.
For an agent-based discovery tool, install the agent on the selected test servers. | Build lead, Migration lead, Network administrator |
| Review the scan results. | Review the scan results for the test servers. If any errors are found, troubleshoot and fix the errors. Document the errors and solutions. You reference this information in the future, and you can add this information to your portfolio runbook. | Build lead, Migration lead, Network administrator |
| Rescan the test servers. | Once the rescan is complete, repeat the scan until there are no errors. | Build lead, Migration lead, Network administrator |
## Related resources
<a name="get-started-with-automated-portfolio-discovery-resources"></a>
**AWS resources**
+ [Application portfolio assessment guide for AWS Cloud migration](https://docs.aws.amazon.com/prescriptive-guidance/latest/application-portfolio-assessment-guide/introduction.html)
+ [Discovery, Planning, and Recommendation migration tools](https://aws.amazon.com/prescriptive-guidance/migration-tools/migration-discovery-tools/)
**Deployment guides for commonly selected discovery tools**
+ [Deploy the RN150 virtual appliance](https://docs.flexera.com/foundationcloudscape/ug/Content/helplibrary/FCGS_QSG_DeployRN150.htm) (Flexera documentation)
+ [Gatherer Installation](https://www.modelizeit.com/documentation/ADC-Gatherer-Install.html) (modelizeIT documentation)
+ [On-Prem Analysis Server Installation](https://www.modelizeit.com/documentation/RejuvenApptor-Install.html) (modelizeIT documentation)
## Additional information
<a name="get-started-with-automated-portfolio-discovery-additional"></a>
**Considerations for selecting an automated discovery tool**
Each discovery tool has benefits and limitations. When selecting the appropriate tool for your use case, consider the following:
+ Select a discovery tool that can collect most, if not all, of the metadata you need to achieve your portfolio assessment goal.
+ Identify any metadata you need to gather manually because the tool doesn’t support it.
+ Provide the discovery tool requirements to stakeholders so they can review and assess the tool based on their internal security and compliance requirements, such as server, network, and credential requirements.
  + Does the tool require that you install an agent in the in-scope workload?
  + Does the tool require that you set up a virtual appliance in your environment?
+ Determine your data residency requirements. Some organizations don’t want to store their data outside of their environment. To address this, you might need to install some components of the tool in the on-premises environment.
+ Make sure the tool supports the operating system (OS) and OS version of the in-scope workload.
+ Determine whether your portfolio includes mainframe, mid-range, and legacy servers. Most of the discovery tools can detect these workloads as dependencies, but some tools might not be able to get device details, such as utilization and server dependencies. Device42 and modernizeIT discovery tools both support mainframe and mid-range servers.
## Attachments
<a name="attachments-8c9d84de-e84a-4b0c-bcaa-389cd90be1f0"></a>
To access additional content that is associated with this document, download and unzip the following file: [attachment.zip](samples/p-attach/8c9d84de-e84a-4b0c-bcaa-389cd90be1f0/attachments/attachment.zip)
