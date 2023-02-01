# Space Pirates Server

## Instructions to upload docker image to ECR

### Retrieve an authentication token and authenticate your Docker client to your registry.

1. First assume the correct role for permision to upload to ECR

```
aws sts assume-role --role-arn arn:aws:iam::045314409243:role/ecr-image-uploader --role-session-name "RoleSession1" --profile rockzombie2 > assume-role-output.txt
```

2. The `assume-role-output.txt` file should contain the credentials you need to login with. Configure the AWS CLI to use those credentials by editing the ~/.aws/credentials file.

3. Run `aws sts get-caller-identity --profile rockzombie2' to verify that you have AWS CLI configured correctly.

4. Run the following command to log in to Docker using the role.

```
aws ecr get-login-password --region us-west-2 --profile ecr-image-uploader | docker login --username AWS --password-stdin 045314409243.dkr.ecr.us-west-2.amazonaws.com
```

### Build your Docker image using the following command

```
docker build -t space-pirates-server .
```

### After the build completes, tag your image so you can push the image to this repository:

```
docker tag space-pirates-server:latest 045314409243.dkr.ecr.us-west-2.amazonaws.com/space-pirates-server:latest
```

### Run the following command to push this image to your newly created AWS repository:

```
docker push 045314409243.dkr.ecr.us-west-2.amazonaws.com/space-pirates-server:latest
```
